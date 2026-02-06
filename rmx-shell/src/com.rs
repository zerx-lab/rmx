use std::ffi::c_void;

use windows::core::*;
use windows::Win32::Foundation::*;
use windows::Win32::System::Com::*;

use crate::menu::RmxContextMenu;

#[implement(IClassFactory)]
pub struct ClassFactory;

impl IClassFactory_Impl for ClassFactory_Impl {
    fn CreateInstance(
        &self,
        punkouter: Option<&IUnknown>,
        riid: *const GUID,
        ppvobject: *mut *mut c_void,
    ) -> Result<()> {
        unsafe {
            if ppvobject.is_null() {
                return Err(E_POINTER.into());
            }
            *ppvobject = std::ptr::null_mut();

            if punkouter.is_some() {
                return Err(CLASS_E_NOAGGREGATION.into());
            }

            let menu = RmxContextMenu::new();
            let unknown: IUnknown = menu.into();
            let hr = unknown.query(&*riid, ppvobject);
            if hr.is_ok() {
                Ok(())
            } else {
                Err(hr.into())
            }
        }
    }

    fn LockServer(&self, flock: BOOL) -> Result<()> {
        if flock.as_bool() {
            crate::increment_lock_count();
        } else {
            crate::decrement_lock_count();
        }
        Ok(())
    }
}
