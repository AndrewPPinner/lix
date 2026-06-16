use std::{ffi::{CStr, CString, c_void}, os::raw::c_char, slice, time::Duration};

use lix_sdk::{CreateBranchOptions, CreateBranchResult, Lix, OpenLixOptions, open_lix};
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
pub extern "C" fn get_active_branch(ptr: *mut LixSession, out_buf: *mut u8, buf_len: usize) -> usize {
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

type CActionCompleteFn<T> = extern "C" fn(T, *mut c_void);
type CActionCompletePtrFn<T> = extern "C" fn(*mut T);

//Update to not be create branch and something that is !Send and async safe
// #[unsafe(no_mangle)]
// pub extern "C" fn create_branch(ptr: *mut LixSession, out_buf: *mut u8, buf_len: usize, context: *mut c_void, callback: CActionCompleteFn<usize>) -> bool {
//     let Some(wrapper) = (unsafe { ptr.as_mut() }) else { return false; };
//     let session_addr = ptr as usize;
//     let cb = callback;
//     let ctx_addr = context as usize;

//     wrapper.runtime.spawn(async move {
//         let session = unsafe { &mut *(session_addr as *mut LixSession) };
//         let ctx = unsafe { &mut *(ctx_addr as *mut c_void) };
//         let reciept = session.engine.create_branch(CreateBranchOptions{ id: None, name: "Name".to_string(), from_commit_id: None }).await.unwrap();
//         let commit_id = reciept.commit_id.as_bytes();
//         let name = reciept.name.as_bytes();
//         let id = reciept.id.as_bytes();
//         let size = commit_id.len() + name.len() + id.len() + 2;

//         if size > buf_len {
//             cb(0, ctx);
//             return;
//         }

//         unsafe {
//             let mut offset = 0;
//             std::ptr::copy_nonoverlapping(
//                 commit_id.as_ptr(),
//                 out_buf.add(offset),
//                 commit_id.len()
//             );
//             *out_buf.add(offset) = b'|';
//             offset += 1;

//             std::ptr::copy_nonoverlapping(
//                 name.as_ptr(),
//                 out_buf,
//                 name.len()
//             );
//             *out_buf.add(offset) = b'|';
//             offset += 1;

//             std::ptr::copy_nonoverlapping(
//                 id.as_ptr(),
//                 out_buf.add(offset),
//                 id.len(),
//             );
//             offset += id.len();
//         }

//         cb(size, ctx);
//     });

//     return true;
// }

#[unsafe(no_mangle)]
pub extern "C" fn create_branch(ptr: *mut LixSession, name_ptr: *const u8, name_len: usize, out_buf: *mut u8, buf_len: usize) -> usize {
    let Some(wrapper) = (unsafe { ptr.as_mut() }) else { return 0; };
    if out_buf.is_null() { return 0; }
    let Ok(name) = (unsafe { str::from_utf8(slice::from_raw_parts(name_ptr, name_len)) }) else {
        return 0;
    };
    let Ok(reciept) = wrapper.runtime.block_on(wrapper.engine.create_branch(CreateBranchOptions{ id: None, name: name.to_owned(), from_commit_id: None })) else {
        return 0;
    };
    let commit_id = reciept.commit_id.as_bytes();
    let name = reciept.name.as_bytes();
    let id = reciept.id.as_bytes();
    let size = commit_id.len() + name.len() + id.len() + 2;

    if size > buf_len {
        return 0;
    }

    unsafe {
        let mut offset = 0;
        std::ptr::copy_nonoverlapping(
            commit_id.as_ptr(),
            out_buf.add(offset),
            commit_id.len()
        );
        offset += commit_id.len();
        *out_buf.add(offset) = b'|';
        offset += 1;

        std::ptr::copy_nonoverlapping(
            name.as_ptr(),
            out_buf.add(offset),
            name.len()
        );
        offset += name.len();
        *out_buf.add(offset) = b'|';
        offset += 1;

        std::ptr::copy_nonoverlapping(
            id.as_ptr(),
            out_buf.add(offset),
            id.len(),
        );
        offset += id.len();
    }

    return size;
}
