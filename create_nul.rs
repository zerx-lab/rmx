use std::os::windows::ffi::OsStrExt;
use std::ffi::OsStr;

fn main() {
    let path = r"\?\C:\Users\zero\Desktop\code\axon_api\nul";
    let wide: Vec<u16> = OsStr::new(path).encode_wide().chain(std::iter::once(0)).collect();
    
    unsafe {
        let handle = windows_sys::Win32::Storage::FileSystem::CreateFileW(
            wide.as_ptr(),
            0x40000000, // GENERIC_WRITE
            0,
            std::ptr::null(),
            2, // CREATE_ALWAYS
            0x80, // FILE_ATTRIBUTE_NORMAL
            std::ptr::null_mut(),
        );
        
        if handle != -1isize as *mut _ {
            windows_sys::Win32::Foundation::CloseHandle(handle);
            println!("Created successfully");
        } else {
            println!("Failed to create: {}", std::io::Error::last_os_error());
        }
    }
}
