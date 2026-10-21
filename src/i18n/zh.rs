//! Chinese interface strings.
//!
//! These are the strings rmx showed before the interface could be translated,
//! byte for byte, so a Chinese user sees no change.

use super::Strings;

pub static STRINGS: Strings = Strings {
    deleting: "正在删除...",
    delete_complete: "删除完成",
    delete_complete_errors: "删除完成（有错误）",
    done_error_one: "完成，{} 个错误",
    done_errors_many: "完成，{} 个错误",
    completed: "已完成",
    deleted_dirs_one: "已删除 {} / {} 个目录",
    deleted_dirs_many: "已删除 {} / {} 个目录",
    copy_errors: "复制错误",
    close: "关闭",
    cancel: "取消",

    confirm_deletion: "确认删除",
    confirm_warning: "此操作不可撤销，文件不会进入回收站",
    items_selected: "已选择 {} 项",
    files_one: "1 个文件",
    files_many: "{} 个文件",
    dirs_one: "{} 个目录",
    dirs_many: "{} 个目录",
    item_separator: "，",
    dont_ask_again: "下次不再确认",
    delete: "删除",

    unlock_files: "文件解锁",
    unlock_help: "帮助你解锁被其他进程占用的文件或文件夹",
    unlocking: "正在解锁...",
    terminating_locking: "正在终止占用进程",
    unlock_successful: "解锁成功",
    terminated_one: "已终止 {} 个占用进程",
    terminated_many: "已终止 {} 个占用进程",
    some_unlocks_failed: "部分解锁失败",
    succeeded_failed: "成功 {} 个，失败 {} 个",
    unlock_following_one: "将对以下 {} 个文件/文件夹进行解锁",
    unlock_following_many: "将对以下 {} 个文件/文件夹进行解锁",
    file_folder_name: "文件/文件夹名称",
    name: "名称",
    path: "路径",
    status: "状态",
    status_unlocking: "解锁中",
    status_pending: "待解锁",
    locked_by: "{} 被以下程序锁定",
    process: "进程",
    reason: "失败原因",
    unlock_all: "全部解锁",
    processing: "正在处理，请稍候...",
    ok: "好的",

    no_locked_files: "未检测到文件被占用，无需解锁",

    deletion_progress: "删除进度",
};
