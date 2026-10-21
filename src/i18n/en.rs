//! English interface strings.
//!
//! The wording comes from the English interface the fork shipped before the
//! translation was moved into the tables.

use super::Strings;

pub static STRINGS: Strings = Strings {
    deleting: "Deleting...",
    delete_complete: "Deletion complete",
    delete_complete_errors: "Deletion complete (with errors)",
    done_error_one: "Done, 1 error",
    done_errors_many: "Done, {} errors",
    completed: "Completed",
    deleted_dirs_one: "Deleted {} / {} directory",
    deleted_dirs_many: "Deleted {} / {} directories",
    copy_errors: "Copy errors",
    close: "Close",
    cancel: "Cancel",

    confirm_deletion: "Confirm deletion",
    confirm_warning: "This action cannot be undone. Files will not go to the Recycle Bin.",
    items_selected: "{} items selected",
    files_one: "1 file",
    files_many: "{} files",
    dirs_one: "1 directory",
    dirs_many: "{} directories",
    item_separator: ", ",
    dont_ask_again: "Don't ask again",
    delete: "Delete",

    unlock_files: "Unlock files",
    unlock_help: "Helps you unlock files or folders locked by other processes",
    unlocking: "Unlocking...",
    terminating_locking: "Terminating locking processes",
    unlock_successful: "Unlock successful",
    terminated_one: "Terminated 1 locking process",
    terminated_many: "Terminated {} locking processes",
    some_unlocks_failed: "Some unlocks failed",
    succeeded_failed: "Succeeded: {}, failed: {}",
    unlock_following_one: "The following 1 file/folder will be unlocked",
    unlock_following_many: "The following {} files/folders will be unlocked",
    file_folder_name: "File/Folder Name",
    name: "Name",
    path: "Path",
    status: "Status",
    status_unlocking: "Unlocking",
    status_pending: "Pending",
    locked_by: "{} is locked by the following processes",
    process: "Process",
    reason: "Failure reason",
    unlock_all: "Unlock all",
    processing: "Processing, please wait...",
    ok: "OK",

    no_locked_files: "No locked files found, nothing to unlock.",

    deletion_progress: "Deletion progress",
};
