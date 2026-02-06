#![allow(non_snake_case)]

mod com;
mod menu;
mod registry;

use std::ffi::c_void;
use std::sync::atomic::{AtomicPtr, AtomicUsize, Ordering};

use windows::core::*;
use windows::Win32::Foundation::*;
use windows::Win32::System::Com::*;
use windows::Win32::System::SystemServices::*;

use com::ClassFactory;

pub static DLL_INSTANCE: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());
static LOCK_COUNT: AtomicUsize = AtomicUsize::new(0);
static OBJECT_COUNT: AtomicUsize = AtomicUsize::new(0);

pub const CLSID_RMX_CONTEXT_MENU: GUID = GUID::from_u128(0x8A5B2C4D_6E7F_4A8B_9C0D_1E2F3A4B5C6D);

pub fn get_dll_instance() -> HMODULE {
    HMODULE(DLL_INSTANCE.load(Ordering::SeqCst))
}

pub fn increment_object_count() {
    OBJECT_COUNT.fetch_add(1, Ordering::SeqCst);
}

pub fn decrement_object_count() {
    OBJECT_COUNT.fetch_sub(1, Ordering::SeqCst);
}

pub fn increment_lock_count() {
    LOCK_COUNT.fetch_add(1, Ordering::SeqCst);
}

pub fn decrement_lock_count() {
    LOCK_COUNT.fetch_sub(1, Ordering::SeqCst);
}

#[no_mangle]
extern "system" fn DllMain(hinstance: HMODULE, reason: u32, _reserved: *mut c_void) -> BOOL {
    match reason {
        DLL_PROCESS_ATTACH => {
            DLL_INSTANCE.store(hinstance.0, Ordering::SeqCst);
            TRUE
        }
        DLL_PROCESS_DETACH => TRUE,
        _ => TRUE,
    }
}

#[no_mangle]
extern "system" fn DllGetClassObject(
    rclsid: *const GUID,
    riid: *const GUID,
    ppv: *mut *mut c_void,
) -> HRESULT {
    unsafe {
        if ppv.is_null() {
            return E_POINTER;
        }
        *ppv = std::ptr::null_mut();

        if *rclsid != CLSID_RMX_CONTEXT_MENU {
            return CLASS_E_CLASSNOTAVAILABLE;
        }

        let factory: IClassFactory = ClassFactory.into();
        factory.query(&*riid, ppv)
    }
}

#[no_mangle]
extern "system" fn DllCanUnloadNow() -> HRESULT {
    if OBJECT_COUNT.load(Ordering::SeqCst) == 0 && LOCK_COUNT.load(Ordering::SeqCst) == 0 {
        S_OK
    } else {
        S_FALSE
    }
}

#[no_mangle]
extern "system" fn DllRegisterServer() -> HRESULT {
    match registry::register_server() {
        Ok(()) => S_OK,
        Err(_) => E_FAIL,
    }
}

#[no_mangle]
extern "system" fn DllUnregisterServer() -> HRESULT {
    match registry::unregister_server() {
        Ok(()) => S_OK,
        Err(_) => E_FAIL,
    }
}
