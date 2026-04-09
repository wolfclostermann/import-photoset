mod config;
mod importer;
mod scanner;

use anyhow::Result;
use config::Config;
use inquire::{Confirm, MultiSelect, Select};

#[derive(Clone, Copy, PartialEq)]
enum Mode {
    Import,
    Delete,
}

impl Mode {
    fn action(&self) -> &'static str {
        match self {
            Mode::Import => "Import",
            Mode::Delete => "Delete",
        }
    }
    fn action_lower(&self) -> &'static str {
        match self {
            Mode::Import => "import",
            Mode::Delete => "delete",
        }
    }
}

fn main() -> Result<()> {
    let mode = if std::env::args().any(|a| a == "--delete") {
        Mode::Delete
    } else {
        Mode::Import
    };

    let config = Config::default();

    // If exactly one camera card is present, skip the menu entirely
    let drives = scanner::find_external_drives();
    let camera_cards: Vec<usize> = drives
        .iter()
        .enumerate()
        .filter(|(_, d)| d.path.join("DCIM").is_dir())
        .map(|(i, _)| i)
        .collect();

    if camera_cards.len() == 1 {
        let drive = &drives[camera_cards[0]];
        println!("Card detected: {}", drive.display_name());
        return run_on_drive(drive, &config, mode);
    }

    main_menu(&config, mode)
}

fn main_menu(config: &Config, mode: Mode) -> Result<()> {
    let label = format!("{} photos from card", mode.action());
    loop {
        let choice = Select::new(
            "Import Photoset",
            vec![label.as_str(), "Quit"],
        )
        .prompt()?;

        if choice == label.as_str() {
            if let Err(e) = select_drive_and_run(config, mode) {
                eprintln!("Error: {e}");
            }
        } else {
            break;
        }
    }
    Ok(())
}

fn select_drive_and_run(config: &Config, mode: Mode) -> Result<()> {
    let drives = scanner::find_external_drives();
    if drives.is_empty() {
        println!("No external drives detected.");
        return Ok(());
    }

    let drive_names: Vec<String> = drives.iter().map(|d| d.display_name()).collect();
    let chosen = Select::new("Select a drive:", drive_names.clone()).prompt()?;
    let drive = &drives[drive_names.iter().position(|n| n == &chosen).unwrap()];

    run_on_drive(drive, config, mode)
}

fn run_on_drive(drive: &scanner::Drive, config: &Config, mode: Mode) -> Result<()> {
    println!("Scanning {} for photosets...", drive.name);
    let photosets = scanner::scan_for_photosets(drive)?;

    if photosets.is_empty() {
        println!("No photosets found.");
        return Ok(());
    }

    println!("Found {} photoset(s).", photosets.len());

    // Select photosets
    let all_label = format!("← {} all", mode.action());
    let mut set_names: Vec<String> = photosets.iter().map(|p| p.display_name()).collect();
    set_names.push(all_label.clone());

    let prompt_label = format!("Select photosets to {}:", mode.action_lower());
    let chosen_names = MultiSelect::new(&prompt_label, set_names)
        .with_help_message("↑↓ move  space select  enter confirm")
        .prompt()?;

    if chosen_names.is_empty() {
        println!("Nothing selected.");
        return Ok(());
    }

    let select_all = chosen_names.contains(&all_label);
    let selected: Vec<&scanner::Photoset> = if select_all {
        photosets.iter().collect()
    } else {
        photosets
            .iter()
            .filter(|p| chosen_names.contains(&p.display_name()))
            .collect()
    };

    match mode {
        Mode::Import => import_selected(&selected, config),
        Mode::Delete => delete_selected(&selected),
    }
}

fn import_selected(selected: &[&scanner::Photoset], config: &Config) -> Result<()> {
    println!();
    println!("  Will import {} photoset(s):", selected.len());
    for p in selected {
        println!("    {}  →  {}", p.date, config.photoset_path(&p.date).display());
    }
    println!();

    let confirm = Confirm::new("Proceed with import?")
        .with_default(true)
        .prompt()?;
    if !confirm {
        return Ok(());
    }

    for (i, photoset) in selected.iter().enumerate() {
        println!();
        println!(
            "[{}/{}] Importing {}  ({} shots, {} files)",
            i + 1,
            selected.len(),
            photoset.date,
            photoset.shot_count(),
            photoset.files.len()
        );
        match importer::import_photoset(photoset, config) {
            Ok(result) => {
                println!("  Done — {} copied, {} already present", result.copied, result.skipped);
                println!("  Destination: {}", result.dest_dir.display());
            }
            Err(e) => eprintln!("  Failed: {e}"),
        }
    }

    println!();
    println!("Import complete.");

    delete_after_import_menu(selected, config)
}

fn delete_after_import_menu(photosets: &[&scanner::Photoset], config: &Config) -> Result<()> {
    let ask = Confirm::new("Delete imported files from the card?")
        .with_default(false)
        .prompt()?;
    if !ask {
        return Ok(());
    }

    println!("Verifying imports before deletion...");
    let mut all_ok = true;
    for photoset in photosets {
        print!("  {} ... ", photoset.date);
        let _ = std::io::Write::flush(&mut std::io::stdout());
        match importer::verify_import(photoset, config) {
            Ok(missing) if missing.is_empty() => println!("ok"),
            Ok(missing) => {
                println!("FAILED ({} file(s) missing or size mismatch)", missing.len());
                for m in &missing {
                    eprintln!("    {}", m.display());
                }
                all_ok = false;
            }
            Err(e) => {
                println!("FAILED ({})", e);
                all_ok = false;
            }
        }
    }

    if !all_ok {
        eprintln!("Verification failed — files will NOT be deleted from the card.");
        return Ok(());
    }

    println!("All files verified.");
    println!();

    do_delete(photosets)
}

fn delete_selected(selected: &[&scanner::Photoset]) -> Result<()> {
    println!();
    println!("  Will delete {} photoset(s) from card:", selected.len());
    for p in selected {
        println!("    {}  ({} shots, {} files)", p.date, p.shot_count(), p.files.len());
    }
    println!();

    let total_files: usize = selected.iter().map(|p| p.files.len()).sum();
    let confirm = Confirm::new(&format!(
        "Permanently delete {} file(s) from the card? This cannot be undone.",
        total_files
    ))
    .with_default(false)
    .prompt()?;
    if !confirm {
        return Ok(());
    }

    do_delete(selected)
}

fn do_delete(photosets: &[&scanner::Photoset]) -> Result<()> {
    let mut deleted = 0usize;
    for photoset in photosets {
        match importer::delete_from_card(photoset) {
            Ok(n) => {
                println!("  {}  — {} file(s) deleted", photoset.date, n);
                deleted += n;
            }
            Err(e) => eprintln!("  {}  — FAILED: {}", photoset.date, e),
        }
    }
    println!("Deleted {} file(s) from card.", deleted);
    Ok(())
}
