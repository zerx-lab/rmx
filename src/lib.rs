//! rmx - Fast parallel directory deletion for Windows
//!
//! This library provides high-performance directory deletion using:
//! - POSIX semantics for immediate namespace removal (Windows 10 1607+)
//! - Parallel deletion with dependency-aware scheduling
//! - Automatic retry for locked files with exponential backoff
//! - Long path support (>260 characters)

pub mod broker;
pub mod error;
pub mod safety;
pub mod tree;
pub mod winapi;
pub mod worker;
