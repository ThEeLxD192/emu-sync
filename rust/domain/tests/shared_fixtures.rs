use emusync_domain::{
    execution::parse_arguments,
    library::clean_game_title,
    sync::{Decision, LocalNewerAction, plan_sync, remote_path_segments},
};

#[test]
fn shared_reference_examples_match_independent_expected_values() {
    for line in include_str!("fixtures/comparison.tsv")
        .lines()
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
    {
        let fields: Vec<_> = line.split('\t').collect();
        let actual = match fields[0] {
            "conflict" => {
                let time = |s: &str| s.parse::<i128>().ok().map(|ms| ms * 1_000_000);
                let action = if fields[3] == "UPLOAD" {
                    LocalNewerAction::Upload
                } else {
                    LocalNewerAction::ResolveConflict
                };
                match plan_sync(time(fields[1]), time(fields[2]), 5, action) {
                    Decision::InSync => "IN_SYNC",
                    Decision::UploadLocal => "UPLOAD_LOCAL",
                    Decision::DownloadCloud => "DOWNLOAD_CLOUD",
                    Decision::Conflict => "CONFLICT",
                }
                .to_owned()
            }
            "tracked" => {
                use emusync_domain::sync::{Checkpoint, RemoteState, plan_tracked};
                let remote = RemoteState {
                    id: "id".into(),
                    modified: 0,
                    sha256: (fields[2] != "null").then(|| fields[2].into()),
                    version: Some(fields[3].into()),
                };
                let previous = Checkpoint {
                    local_sha256: "old".into(),
                    remote_id: "id".into(),
                    remote_modified: 0,
                    remote_sha256: Some("old".into()),
                    remote_version: Some("1".into()),
                };
                match plan_tracked(
                    Some(fields[1]),
                    Some(&remote),
                    (fields[4] == "yes").then_some(&previous),
                ) {
                    Decision::InSync => "IN_SYNC",
                    Decision::UploadLocal => "UPLOAD_LOCAL",
                    Decision::DownloadCloud => "DOWNLOAD_CLOUD",
                    Decision::Conflict => "CONFLICT",
                }
                .into()
            }
            "directory" => {
                use emusync_domain::sync::{CloudFile, LocalFile, plan_directory};
                let local: Vec<_> = fields[1]
                    .split(';')
                    .map(|item| {
                        let f: Vec<_> = item.split('@').collect();
                        LocalFile {
                            path: f[0].into(),
                            modified: f[1].parse::<i128>().unwrap() * 1_000_000,
                        }
                    })
                    .collect();
                let cloud: Vec<_> = fields[2]
                    .split(';')
                    .map(|item| {
                        let f: Vec<_> = item.split('@').collect();
                        CloudFile {
                            id: f[0].into(),
                            path: f[1].into(),
                            modified: f[2].parse::<i128>().unwrap() * 1_000_000,
                        }
                    })
                    .collect();
                plan_directory(&local, &cloud, LocalNewerAction::ResolveConflict)
                    .iter()
                    .map(|f| {
                        let decision = match f.decision {
                            Decision::InSync => "IN_SYNC",
                            Decision::UploadLocal => "UPLOAD_LOCAL",
                            Decision::DownloadCloud => "DOWNLOAD_CLOUD",
                            Decision::Conflict => "CONFLICT",
                        };
                        format!(
                            "{}:{}:{}:{}",
                            f.path,
                            decision,
                            f.cloud.as_ref().map_or("-", |c| c.id.as_str()),
                            f.duplicates
                                .iter()
                                .map(|c| c.id.as_str())
                                .collect::<Vec<_>>()
                                .join(",")
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(";")
            }
            "remote" => {
                remote_path_segments(fields[1]).map_or_else(|| "INVALID".into(), |v| v.join("|"))
            }
            "arguments" => {
                parse_arguments(fields[1]).map_or_else(|| "INVALID".into(), |v| v.join("|"))
            }
            "title" => clean_game_title(fields[1]),
            _ => panic!("unknown fixture: {line}"),
        };
        assert_eq!(actual, fields[fields.len() - 1], "{line}");
    }
}
