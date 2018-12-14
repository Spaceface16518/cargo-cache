// Copyright 2017-2018 Matthias Krüger. See the COPYRIGHT
// file at the top-level directory of this distribution.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use std::cmp::Ordering;
use std::fs;
use std::path::PathBuf;

use crate::cache::dircache::DirCache;
use crate::top_items::common::{dir_exists, TOP_CRATES_SPACING};
use humansize::{file_size_opts, FileSize};
use rayon::iter::*;
use walkdir::WalkDir;

#[derive(Clone, Debug)]
struct FileDesc {
    path: PathBuf,
    name: String,
    size: u64,
}

impl FileDesc {
    fn new_from_git_bare(path: &PathBuf) -> Self {
        let last_item = path.file_name().unwrap().to_str().unwrap().to_string();
        let mut i = last_item.split('-').collect::<Vec<_>>();
        i.pop();
        let name = i.join("-");

        let walkdir = WalkDir::new(path.display().to_string());

        let size = walkdir
            .into_iter()
            .map(|e| e.unwrap().path().to_owned())
            .filter(|f| f.exists())
            .collect::<Vec<_>>()
            .par_iter()
            .map(|f| {
                fs::metadata(f)
                    .unwrap_or_else(|_| {
                        panic!("Failed to get metadata of file '{}'", &path.display())
                    })
                    .len()
            })
            .sum();

        Self {
            path: path.into(),
            name,
            size,
        }
    } // fn new_from_git_bare()
}

#[derive(Clone, Debug, Eq)]
pub(crate) struct RepoInfo {
    name: String,
    size: u64,
    counter: u32,
    total_size: u64, // sorted by this
}

impl RepoInfo {
    fn new(path: &PathBuf, counter: u32, total_size: u64) -> Self {
        let size: u64;
        let name: String;
        if path.exists() {
            // get the string
            let name_tmp = path.file_name().unwrap().to_str().unwrap().to_string();
            // remove the hash from the path (mdbook-e6b52d90d4246c70 => mdbook)
            let mut tmp_name = name_tmp.split('-').collect::<Vec<_>>();
            tmp_name.pop(); // remove the hash
            name = tmp_name.join("-");
            size = fs::metadata(&path)
                .unwrap_or_else(|_| panic!("Failed to get metadata of file '{}'", &path.display()))
                .len();
        } else {
            // tests
            name = path
                .file_name()
                .unwrap()
                .to_os_string()
                .into_string()
                .unwrap();
            size = 0;
        }
        Self {
            name,
            size,
            counter,
            total_size,
        }
    }
}

impl PartialOrd for RepoInfo {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for RepoInfo {
    fn cmp(&self, other: &Self) -> Ordering {
        self.total_size.cmp(&other.total_size)
    }
}

impl PartialEq for RepoInfo {
    fn eq(&self, other: &Self) -> bool {
        self.total_size == other.total_size
    }
}

fn file_desc_from_path(cache: &mut DirCache) -> Vec<FileDesc> {
    // get list of package all "...\.crate$" files and sort it
    cache
        .git_repos_bare
        .bare_repo_folders() // bad
        .iter()
        .map(|path| FileDesc::new_from_git_bare(path))
        .collect::<Vec<_>>()
}

fn stats_from_file_desc_list(file_descs: Vec<FileDesc>) -> Vec<RepoInfo> {
    struct Pair {
        current: Option<FileDesc>,
        previous: Option<FileDesc>,
    }
    // take our list of file information and calculate the actual stats
    let mut out: Vec<RepoInfo> = Vec::new();
    let mut repoinfo: RepoInfo = RepoInfo::new(&PathBuf::from("ERROR 1/err1"), 0, 0);
    let mut counter: u32 = 0; // how many of a crate do we have
    let mut total_size: u64 = 0; // total size of these crates

    // iterate over the files
    let mut iter = file_descs.into_iter();

    let mut state = Pair {
        current: None,
        previous: None,
    };

    // start looping
    state.previous = state.current;
    state.current = iter.next();

    // loop until .previous and .current are None which means we are at the end
    while state.previous.is_some() || state.current.is_some() {
        match &state {
            Pair {
                current: None,
                previous: None,
            } => {
                // we reached the end of the queue
            }

            Pair {
                current: Some(current),
                previous: None,
            } => {
                // this should always be first line ever
                // @TODO(assert that repoinfo is empty)
                // compute line but don't save it
                let current_size = &current.size;
                total_size += current_size;
                counter += 1;

                repoinfo = RepoInfo::new(&current.path, counter, total_size);
            }

            Pair {
                current: Some(current),
                previous: Some(previous),
            } => {
                if current.name == previous.name {
                    // update line but don't save it
                    // @TODO(assert that repoinfo is not empty)

                    let current_size = &current.size;
                    total_size += current_size;
                    counter += 1;

                    repoinfo = RepoInfo::new(&current.path, counter, total_size);
                } else if current.name != previous.name {
                    // save old line
                    //                       // @TODO(assert that repoinfo is not empty)
                    out.push(repoinfo);
                    // reset counters
                    counter = 0;
                    total_size = 0;
                    // and update line
                    let current_size = &current.size;
                    total_size += current_size;
                    counter += 1;

                    repoinfo = RepoInfo::new(&current.path, counter, total_size);
                }
            }

            Pair {
                current: None,
                previous: Some(_previous),
            } => {
                // save old line
                // @TODO assert that repoinfo is not empty
                out.push(repoinfo);
                repoinfo = RepoInfo::new(&PathBuf::from("ERROR 2/err2"), 0, 0);
                // reset counters
                counter = 0;
                total_size = 0;
            }
        };

        // switch and queue next()
        state.previous = state.current;
        state.current = iter.next();
    }

    out
}

pub(crate) fn chkout_list_to_string(limit: u32, mut collections_vec: Vec<RepoInfo>) -> String {
    // sort the RepoINfo Vec in reverse, biggest item first
    collections_vec.sort();
    collections_vec.reverse();
    let mut output = String::new();

    let max_cratename_len = collections_vec
        .iter()
        .take(limit as usize)
        .map(|p| p.name.len())
        .max()
        .unwrap_or(0);

    for repoinfo in collections_vec.into_iter().take(limit as usize) {
        let average_crate_size = (repoinfo.total_size / u64::from(repoinfo.counter))
            .file_size(file_size_opts::DECIMAL)
            .unwrap();
        let avg_string = format!("src avg: {: >9}", average_crate_size);
        output.push_str(&format!(
            "{: <width$} src ckt: {: <3} {: <20} total: {}\n",
            repoinfo.name,
            repoinfo.counter,
            avg_string,
            repoinfo
                .total_size
                .file_size(file_size_opts::DECIMAL)
                .unwrap(),
            width = max_cratename_len + TOP_CRATES_SPACING,
        ));
    }
    output
}

// bare git repos
pub(crate) fn git_repos_bare_stats(path: &PathBuf, limit: u32, mut cache: &mut DirCache) -> String {
    let mut output = String::new();
    // don't crash if the directory does not exist (issue #9)
    if !dir_exists(path) {
        return output;
    }

    output.push_str(&format!(
        "\nSummary of: {} ({} total)\n",
        path.display(),
        cache
            .git_repos_bare
            .total_size()
            .file_size(file_size_opts::DECIMAL)
            .unwrap()
    ));

    let collections_vec = file_desc_from_path(&mut cache);
    let summary: Vec<RepoInfo> = stats_from_file_desc_list(collections_vec);
    let tmp = chkout_list_to_string(limit, summary);

    output.push_str(&tmp);
    output
}

#[cfg(test)]
mod top_crates_git_repos_bare {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn stats_from_file_desc_none() {
        // empty list
        let list: Vec<FileDesc> = Vec::new();
        let stats = stats_from_file_desc_list(list);
        let is = chkout_list_to_string(4, stats);
        let empty = String::new();
        assert_eq!(is, empty);
    }

    #[test]
    fn stats_from_file_desc_one() {
        let fd = FileDesc {
            path: PathBuf::from("crateA"),
            name: "crateA".to_string(),
            size: 1,
        };
        let list_fd: Vec<FileDesc> = vec![fd];
        let list_cb: Vec<RepoInfo> = stats_from_file_desc_list(list_fd);
        let is: String = chkout_list_to_string(1, list_cb);
        let wanted = String::from("crateA    src ckt: 1   src avg:       1 B   total: 1 B\n");
        assert_eq!(is, wanted);
    }

    #[test]
    fn stats_from_file_desc_two() {
        let fd1 = FileDesc {
            path: PathBuf::from("crate-A"),
            name: "crate-A".to_string(),
            size: 1,
        };
        let fd2 = FileDesc {
            path: PathBuf::from("crate-B"),
            name: "crate-B".to_string(),
            size: 2,
        };
        let list_fd: Vec<FileDesc> = vec![fd1, fd2];
        let list_cb: Vec<RepoInfo> = stats_from_file_desc_list(list_fd);
        let is: String = chkout_list_to_string(3, list_cb);

        let mut wanted = String::new();
        for i in &[
            "crate-B    src ckt: 1   src avg:       2 B   total: 2 B\n",
            "crate-A    src ckt: 1   src avg:       1 B   total: 1 B\n",
        ] {
            wanted.push_str(i);
        }
        assert_eq!(is, wanted);
    }

    #[test]
    fn stats_from_file_desc_multiple() {
        let fd1 = FileDesc {
            path: PathBuf::from("crate-A"),
            name: "crate-A".to_string(),
            size: 1,
        };
        let fd2 = FileDesc {
            path: PathBuf::from("crate-B"),
            name: "crate-B".to_string(),
            size: 2,
        };
        let fd3 = FileDesc {
            path: PathBuf::from("crate-C"),
            name: "crate-C".to_string(),
            size: 10,
        };
        let fd4 = FileDesc {
            path: PathBuf::from("crate-D"),
            name: "crate-D".to_string(),
            size: 6,
        };
        let fd5 = FileDesc {
            path: PathBuf::from("crate-E"),
            name: "crate-E".to_string(),
            size: 4,
        };
        let list_fd: Vec<FileDesc> = vec![fd1, fd2, fd3, fd4, fd5];
        let list_cb: Vec<RepoInfo> = stats_from_file_desc_list(list_fd);

        let is: String = chkout_list_to_string(6, list_cb);

        let mut wanted = String::new();
        for i in &[
            "crate-C    src ckt: 1   src avg:      10 B   total: 10 B\n",
            "crate-D    src ckt: 1   src avg:       6 B   total: 6 B\n",
            "crate-E    src ckt: 1   src avg:       4 B   total: 4 B\n",
            "crate-B    src ckt: 1   src avg:       2 B   total: 2 B\n",
            "crate-A    src ckt: 1   src avg:       1 B   total: 1 B\n",
        ] {
            wanted.push_str(i);
        }
        assert_eq!(is, wanted);
    }

    #[test]
    fn stats_from_file_desc_same_name_2_one() {
        let fd1 = FileDesc {
            path: PathBuf::from("crate-A"),
            name: "crate-A".to_string(),
            size: 3,
        };
        let fd2 = FileDesc {
            path: PathBuf::from("crate-A"),
            name: "crate-A".to_string(),
            size: 3,
        };

        let list_fd: Vec<FileDesc> = vec![fd1, fd2];
        let list_cb: Vec<RepoInfo> = stats_from_file_desc_list(list_fd);
        let is: String = chkout_list_to_string(2, list_cb);
        let wanted = String::from("crate-A    src ckt: 2   src avg:       3 B   total: 6 B\n");
        assert_eq!(is, wanted);
    }

    #[test]
    fn stats_from_file_desc_same_name_3_one() {
        let fd1 = FileDesc {
            path: PathBuf::from("crate-A"),
            name: "crate-A".to_string(),
            size: 3,
        };
        let fd2 = FileDesc {
            path: PathBuf::from("crate-A"),
            name: "crate-A".to_string(),
            size: 3,
        };
        let fd3 = FileDesc {
            path: PathBuf::from("crate-A"),
            name: "crate-A".to_string(),
            size: 3,
        };

        let list_fd: Vec<FileDesc> = vec![fd1, fd2, fd3];

        let list_cb: Vec<RepoInfo> = stats_from_file_desc_list(list_fd);
        let is: String = chkout_list_to_string(3, list_cb);
        let wanted = String::from("crate-A    src ckt: 3   src avg:       3 B   total: 9 B\n");
        assert_eq!(is, wanted);
    }

    #[test]
    fn stats_from_file_desc_same_name_3_one_2() {
        let fd1 = FileDesc {
            path: PathBuf::from("crate-A"),
            name: "crate-A".to_string(),
            size: 2,
        };
        let fd2 = FileDesc {
            path: PathBuf::from("crate-A"),
            name: "crate-A".to_string(),
            size: 4,
        };
        let fd3 = FileDesc {
            path: PathBuf::from("crate-A"),
            name: "crate-A".to_string(),
            size: 12,
        };

        let list_fd: Vec<FileDesc> = vec![fd1, fd2, fd3];
        let list_cb: Vec<RepoInfo> = stats_from_file_desc_list(list_fd);
        let is: String = chkout_list_to_string(3, list_cb);
        let wanted = String::from("crate-A    src ckt: 3   src avg:       6 B   total: 18 B\n");
        assert_eq!(is, wanted);
    }

    #[test]
    fn stats_from_file_desc_multi() {
        let fd1 = FileDesc {
            path: PathBuf::from("crate-A"),
            name: "crate-A".to_string(),
            size: 2,
        };
        let fd2 = FileDesc {
            path: PathBuf::from("crate-A"),
            name: "crate-A".to_string(),
            size: 4,
        };
        let fd3 = FileDesc {
            path: PathBuf::from("crate-A"),
            name: "crate-A".to_string(),
            size: 12,
        };

        let fd4 = FileDesc {
            path: PathBuf::from("crate-B"),
            name: "crate-B".to_string(),
            size: 2,
        };
        let fd5 = FileDesc {
            path: PathBuf::from("crate-B"),
            name: "crate-B".to_string(),
            size: 8,
        };

        let fd6 = FileDesc {
            path: PathBuf::from("crate-C"),
            name: "crate-C".to_string(),
            size: 0,
        };
        let fd7 = FileDesc {
            path: PathBuf::from("crate-C"),
            name: "crate-C".to_string(),
            size: 100,
        };

        let fd8 = FileDesc {
            path: PathBuf::from("crate-D"),
            name: "crate-D".to_string(),
            size: 1,
        };

        let list_fd: Vec<FileDesc> = vec![fd1, fd2, fd3, fd4, fd5, fd6, fd7, fd8];
        let list_cb: Vec<RepoInfo> = stats_from_file_desc_list(list_fd);
        let is: String = chkout_list_to_string(5, list_cb);

        let mut wanted = String::new();

        for i in &[
            "crate-C    src ckt: 2   src avg:      50 B   total: 100 B\n",
            "crate-A    src ckt: 3   src avg:       6 B   total: 18 B\n",
            "crate-B    src ckt: 2   src avg:       5 B   total: 10 B\n",
            "crate-D    src ckt: 1   src avg:       1 B   total: 1 B\n",
        ] {
            wanted.push_str(i);
        }
        assert_eq!(is, wanted);
    }
}
#[cfg(all(test, feature = "bench"))]
mod benchmarks {
    use super::*;
    use crate::test::black_box;
    use crate::test::Bencher;

    #[bench]
    fn bench_few(b: &mut Bencher) {
        let fd1 = FileDesc {
            path: PathBuf::from("crate-A"),
            name: "crate-A".to_string(),
            size: 2,
        };
        let fd2 = FileDesc {
            path: PathBuf::from("crate-A"),
            name: "crate-A".to_string(),
            size: 4,
        };
        let fd3 = FileDesc {
            path: PathBuf::from("crate-A"),
            name: "crate-A".to_string(),
            size: 12,
        };

        let fd4 = FileDesc {
            path: PathBuf::from("crate-B"),
            name: "crate-B".to_string(),
            size: 2,
        };
        let fd5 = FileDesc {
            path: PathBuf::from("crate-B"),
            name: "crate-B".to_string(),
            size: 8,
        };

        let fd6 = FileDesc {
            path: PathBuf::from("crate-C"),
            name: "crate-C".to_string(),
            size: 0,
        };
        let fd7 = FileDesc {
            path: PathBuf::from("crate-C"),
            name: "crate-C".to_string(),
            size: 100,
        };

        let fd8 = FileDesc {
            path: PathBuf::from("crate-D"),
            name: "crate-D".to_string(),
            size: 1,
        };

        let list_fd: Vec<FileDesc> = vec![fd1, fd2, fd3, fd4, fd5, fd6, fd7, fd8];

        b.iter(|| {
            let list_fd = list_fd.clone(); // @FIXME  don't?
            let list_cb: Vec<RepoInfo> = stats_from_file_desc_list(list_fd);
            let is: String = chkout_list_to_string(5, list_cb);

            black_box(is);
        });
    }

}
