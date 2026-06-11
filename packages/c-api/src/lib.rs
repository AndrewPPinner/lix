use std::{ffi::CString, os::raw::c_char, time::Duration};

use lix_sdk::{Lix, OpenLixOptions, open_lix};
use tokio::{runtime::{Builder, Runtime}, time::sleep};

pub struct LixSession {
    pub engine: Lix,
    pub runtime: Runtime
}

#[repr(C)]
pub struct CResult<T> {
    is_success: bool,
    data: T
}

#[repr(C)]
pub struct CPtrResult<T> {
    is_success: bool,
    data: *mut T
}

#[unsafe(no_mangle)]
pub extern "C" fn lix_version() -> u32 {
    return 1;
}

#[unsafe(no_mangle)]
pub extern "C" fn open() -> *mut LixSession {
    let rt = Builder::new_multi_thread().enable_all().build().unwrap();
    let lix = rt.block_on(open_lix(OpenLixOptions::default())).unwrap();
    return Box::into_raw(Box::new(LixSession { engine: lix, runtime: rt }));
}

#[unsafe(no_mangle)]
pub extern "C" fn close(ptr: *mut LixSession) -> bool {
    let Some(wrapper) = (unsafe { ptr.as_mut() }) else { return false; };
    let _ = wrapper.runtime.block_on(wrapper.engine.close());
    drop(unsafe { Box::from_raw(wrapper) });
    return true;
}

#[unsafe(no_mangle)]
pub extern "C" fn get_active_branch(ptr: *mut LixSession) -> CPtrResult<c_char> {
    let Some(wrapper) = (unsafe { ptr.as_mut() }) else {
        return CPtrResult { is_success: false, data: std::ptr::null_mut() };
    };
    let Ok(branch_id) = wrapper.runtime.block_on(wrapper.engine.active_branch_id()) else {
        return CPtrResult { is_success: false, data: std::ptr::null_mut() };
    };

    let c_string = CString::new(branch_id).unwrap();
    return CPtrResult { is_success: false, data: c_string.into_raw() };
}

#[unsafe(no_mangle)]
pub extern "C" fn get_active_branch_buffer(ptr: *mut LixSession, out_buf: *mut u8, buf_len: usize) -> usize {
    let Some(wrapper) = (unsafe { ptr.as_mut() }) else {
        return 0;
    };
    if out_buf.is_null() { return 0; }
    let Ok(branch_id) = wrapper.runtime.block_on(wrapper.engine.active_branch_id()) else {
        return 0;
    };

    let bytes = branch_id.as_bytes();
    if bytes.len() > buf_len {
        return 0;
    }
    unsafe {
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), out_buf, bytes.len());
    }

    return bytes.len();
}

#[unsafe(no_mangle)]
pub extern "C" fn free_active_branch(ptr: *mut c_char) -> bool {
    if ptr.is_null() {
        return false;
    }
    drop(unsafe { Box::from_raw(ptr) });
    return true;
}

#[repr(C)]
pub struct Test1Value {
    pub value1: u32,
    pub value2: bool
}

#[unsafe(no_mangle)]
pub extern "C" fn Test1(ptr: *mut LixSession, out_buf: *mut u8, buf_len: usize, limit: usize) -> usize {
    let Some(wrapper) = (unsafe { ptr.as_mut() }) else { return 0; };
    if out_buf.is_null() { return 0; }
    let mut test_vec: Vec<Test1Value> = wrapper.runtime.block_on(async {
        vec![
            Test1Value{ value1: 69, value2: true },
            Test1Value{ value1: 420, value2: false },
        ]
    });

    let value_size = std::mem::size_of::<Test1Value>();
    let actual_limit = std::cmp::min(test_vec.len(), limit);
    let response_slice = &test_vec[..actual_limit];
    let buffer_size_required = value_size * response_slice.len();

    if buf_len < buffer_size_required {
        return 0;
    }

    unsafe {
        std::ptr::copy_nonoverlapping(
            response_slice.as_ptr() as *const u8,
            out_buf,
            buffer_size_required
        );
    }

    return buffer_size_required;
}

type CActionCompleteFn<T> = extern "C" fn(T);
type CActionCompletePtrFn<T> = extern "C" fn(*mut T);

#[unsafe(no_mangle)]
pub extern "C" fn Test2Async(ptr: *mut LixSession, out_buf: *mut u8, buf_len: usize, callback: CActionCompleteFn<u32>) -> bool {
    let Some(wrapper) = (unsafe { ptr.as_mut() }) else { return false; };
    let async_handler = wrapper.runtime.handle().clone();
    let session_addr = ptr as usize;
    let cb = callback;

    async_handler.spawn(async move {
        sleep(Duration::from_secs(3)).await;
        let session = unsafe { &mut *(session_addr as *mut LixSession) };
        session.engine.active_branch_id().await.unwrap();
        cb(69);
    });

    return true;
}
