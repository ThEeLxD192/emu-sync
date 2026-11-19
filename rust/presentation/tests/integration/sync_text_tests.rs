//! Synchronization messages read the same in English as before they were typed, and
//! read in the language asked for.

use emusync_domain::sync::SyncMessage;
use emusync_presentation::i18n::{Language, sync_text, texts};

fn english(message: &SyncMessage) -> String {
    sync_text(texts(Language::English), message)
}

fn spanish(message: &SyncMessage) -> String {
    sync_text(texts(Language::Spanish), message)
}

#[test]
fn english_messages_keep_their_wording() {
    assert_eq!(
        english(&SyncMessage::SyncingGameSaves {
            game: "Celeste".into()
        }),
        "Syncing saves for Celeste..."
    );
    assert_eq!(
        english(&SyncMessage::DriveNotConnected),
        "Google Drive is not connected. Click 'Connect Drive' in the top bar to sign in."
    );
    assert_eq!(
        english(&SyncMessage::Failed {
            detail: "disk full".into()
        }),
        "Error: disk full"
    );
}

#[test]
fn spanish_messages_carry_the_same_values() {
    assert_eq!(
        spanish(&SyncMessage::SyncingGameSaves {
            game: "Celeste".into()
        }),
        "Sincronizando las partidas de Celeste..."
    );
    assert_eq!(
        spanish(&SyncMessage::Detail("os error 13".into())),
        "os error 13",
        "a lower-level error is shown as it came"
    );
}

#[test]
fn several_messages_read_one_per_line() {
    let message = SyncMessage::Several(vec![
        SyncMessage::Detail("first".into()),
        SyncMessage::Several(vec![SyncMessage::UnknownDriveError]),
    ]);

    assert_eq!(english(&message), "first\nUnknown Drive error");
}

#[test]
fn long_folder_lists_show_five_and_count_the_rest() {
    let message = SyncMessage::UnplacedLegacyFiles {
        folders: (1..=7).map(|n| format!("/saves/{n}")).collect(),
        files: vec!["a.sav".into()],
    };

    let text = spanish(&message);

    assert!(text.contains("Carpetas de partida configuradas:\n• /saves/1\n"));
    assert!(text.contains("• /saves/5\n… (2 más)"));
    assert!(!text.contains("/saves/6"));
    assert!(text.contains("Archivos de la nube sin partida local:\n• a.sav"));
}
