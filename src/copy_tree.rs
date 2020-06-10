// Conserve backup system.
// Copyright 2017, 2018, 2019, 2020 Martin Pool.

//! Copy tree contents.
use std::cmp::{Ord, Ordering};
#[allow(unused_imports)]
use snafu::ResultExt;

use crate::stats::CopyStats;
use crate::*;

#[derive(Default, Clone, Debug)]
pub struct CopyOptions {
    pub print_filenames: bool,
    pub measure_first: bool,
}

pub const COPY_DEFAULT: CopyOptions = CopyOptions {
    print_filenames: false,
    measure_first: false,
};

/// Copy files and other entries from one tree to another.
pub fn copy_tree<ST: ReadTree, DT: WriteTree>(
    source: &ST,
    mut dest: DT,
    options: &CopyOptions,
) -> Result<CopyStats> {
    let mut stats = CopyStats::default();
    // This causes us to walk the source tree twice, which is probably an acceptable option
    // since it's nice to see realistic overall progress. We could keep all the entries
    // in memory, and maybe we should, but it might get unreasonably big.
    if options.measure_first {
        ui::set_progress_phase("Measure source tree");
        // TODO: Maybe read all entries for the source tree in to memory now, rather than walking it
        // again a second time? But, that'll potentially use memory proportional to tree size, which
        // I'd like to avoid, and also perhaps make it more likely we grumble about files that were
        // deleted or changed while this is running.
        ui::set_bytes_total(source.size()?.file_bytes);
    }
    ui::set_progress_phase("Copying");
    for entry in source.iter_entries()? {
        let target = "DataScienceatHome";

        // let found = entry.apath().eq("/DataScienceatHome");
        // println!("{:?} == {:?} {:?}", entry.apath(), target, found);
        
        // let ordering = entry.apath().cmp(&target);
        // println!("ordering: {:?}", ordering);

        let subtree: Vec<&str> = entry.apath().split('/').collect();
        println!("subtree is {:?}", subtree[1]);
        let to_be_copied = subtree[1].eq(target);
        println!("to be copied: {:?}", to_be_copied);

        // if found {
        //     let tree = Apath::from(entry.apath().to_owned());
        //     println!("\ntree: {:?}", tree)
        
        //     // match entry.apath().cmp(&target) {

        //     //     // Ordering::Equal => {
        //     //     //     found = true;
        //     //     //     println!("\nDEBUG {:?} target={:?} found={:?}\n", 
        //     //     //     entry.apath(), Apath::from(target), found); 
    
        //     //     //     // target = Apath::from(target);
                    
        //     //     // },
                
        //     //     Ordering::Greater => {
        //     //         println!("\n entry: {:?} target: {:?} \n", 
        //     //         entry.apath(), target); 
        //     //     },

        //     //     _ => {}
        //     // }

        // }

        


        if options.print_filenames {
            crate::ui::println(entry.apath());
        }
        ui::set_progress_file(entry.apath());
        if let Err(e) = match entry.kind() {
            Kind::Dir => {
                stats.directories += 1;
                dest.copy_dir(&entry)
            }
            Kind::File => {
                stats.files += 1;
                dest.copy_file(&entry, source).map(|s| stats += s)
            }
            Kind::Symlink => {
                stats.symlinks += 1;
                dest.copy_symlink(&entry)
            }
            Kind::Unknown => {
                stats.unknown_kind += 1;
                // TODO: Perhaps eventually we could backup and restore pipes,
                // sockets, etc. Or at least count them. For now, silently skip.
                // https://github.com/sourcefrog/conserve/issues/82
                continue;
            }
        } {
            ui::show_error(&e);
            stats.errors += 1;
            continue;
        }
        ui::increment_bytes_done(entry.size().unwrap_or(0));
    }
    ui::clear_progress();
    stats += dest.finish()?;
    // TODO: Merge in stats from the tree iter and maybe the source tree?
    Ok(stats)
}
