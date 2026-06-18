use std::{slice, str};
use tokio::runtime::{Builder, Runtime};
use lix_sdk::{
    Backend, CreateBranchOptions, InMemoryBackend, Lix, SqliteBackend, SwitchBranchOptions, open_lix_with_backend
};

#[repr(C)]
#[expect(missing_debug_implementations)]
pub struct LixSession {
    inner: Box<dyn Session>,
}

struct LixSessionImpl<B> where
    B: Backend + Clone + Send + Sync + 'static,
    for<'a> B::Read<'a>: Send,
    for<'a> B::Write<'a>: Send,
{
    engine: Lix<B>,
    runtime: Runtime,
}

#[repr(C)]
#[derive(Debug)]
pub struct LixOpenOptions {
    pub backend: BackendType,
    pub data: *mut u8,
    pub data_len: usize,
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub enum BackendType {
    Sqlite = 1,
    InMem = 2
}

trait Session: Send {
    fn close(&mut self) -> bool;

    fn get_active_branch(
        &mut self, out_buf: *mut u8,
        buf_len: usize) -> usize;

    fn create_branch(
        &mut self, name_ptr: *const u8, name_len: usize,
        out_buf: *mut u8, buf_len: usize) -> usize;

    fn change_branch(&mut self, branch_id_ptr: *const u8, branch_id_len: usize) -> bool;
}


/// One implementation works for every backend.
impl<B> Session for LixSessionImpl<B> where
    B: Backend + Clone + Send + Sync + 'static,
    for<'a> B::Read<'a>: Send,
    for<'a> B::Write<'a>: Send,
{
    fn close(&mut self) -> bool {
        return self.runtime
            .block_on(self.engine.close())
            .is_ok();
    }

    fn get_active_branch(&mut self, out_buf: *mut u8, buf_len: usize) -> usize {
        if out_buf.is_null() {
            return 0;
        }

        let Ok(branch_id) =
            self.runtime.block_on(self.engine.active_branch_id())
        else {
            return 0;
        };

        let bytes = branch_id.as_bytes();

        if bytes.len() > buf_len {
            return 0;
        }

        unsafe {
            std::ptr::copy_nonoverlapping(
                bytes.as_ptr(),
                out_buf,
                bytes.len(),
            );
        }

        return bytes.len();
    }


    fn create_branch(&mut self, name_ptr: *const u8, name_len: usize, out_buf: *mut u8, buf_len: usize) -> usize {
        if out_buf.is_null() {
            return 0;
        }

        let Ok(name) = (unsafe {
            str::from_utf8(
                slice::from_raw_parts(name_ptr, name_len),
            )
        })
        else {
            return 0;
        };


        let Ok(receipt) = self.runtime.block_on(
            self.engine.create_branch(CreateBranchOptions {
                id: None,
                name: name.to_owned(),
                from_commit_id: None,
            }),
        )
        else {
            return 0;
        };


        let commit_id = receipt.commit_id.as_bytes();
        let name = receipt.name.as_bytes();
        let id = receipt.id.as_bytes();

        let size =
            commit_id.len() +
            name.len() +
            id.len() +
            2;


        if size > buf_len {
            return 0;
        }


        unsafe {
            let mut offset = 0;

            std::ptr::copy_nonoverlapping(
                commit_id.as_ptr(),
                out_buf.add(offset),
                commit_id.len(),
            );

            offset += commit_id.len();

            *out_buf.add(offset) = b'|';
            offset += 1;


            std::ptr::copy_nonoverlapping(
                name.as_ptr(),
                out_buf.add(offset),
                name.len(),
            );

            offset += name.len();

            *out_buf.add(offset) = b'|';
            offset += 1;


            std::ptr::copy_nonoverlapping(
                id.as_ptr(),
                out_buf.add(offset),
                id.len(),
            );
        }

        return size;
    }

    fn change_branch(&mut self, branch_id_ptr: *const u8, branch_id_len: usize) -> bool {
        let Ok(branch_id) = (unsafe {
            str::from_utf8(
                slice::from_raw_parts(branch_id_ptr, branch_id_len),
            )
        }) else { return false; };
        let is_success = self.runtime.block_on(async {
            return self.engine.switch_branch(SwitchBranchOptions{ branch_id: branch_id.to_owned() }).await.is_ok();
        });
        return is_success;
    }

    //Maybe a get branch info?
}

fn make_session<B>(backend: B) -> LixSession
where
    B: Backend + Clone + Send + Sync + 'static,
    for<'a> B::Read<'a>: Send,
    for<'a> B::Write<'a>: Send,
{
    let runtime = Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();


    let engine = runtime
        .block_on(open_lix_with_backend(backend))
        .unwrap();


    LixSession {
        inner: Box::new(LixSessionImpl {
            engine,
            runtime,
        }),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn lix_version() -> u32 {
    1
}


#[unsafe(no_mangle)]
pub extern "C" fn open(
    ptr: *mut LixOpenOptions,
) -> *mut LixSession {
    let Some(options) = (unsafe { ptr.as_mut() }) else {
        return std::ptr::null_mut();
    };


    let session = match options.backend {
        BackendType::Sqlite => {
            let Ok(path) = (unsafe {
                str::from_utf8(
                    slice::from_raw_parts(
                        options.data,
                        options.data_len,
                    ),
                )
            })
            else {
                return std::ptr::null_mut();
            };


            let Ok(backend) = SqliteBackend::open(path) else {
                return std::ptr::null_mut();
            };


            make_session(backend)
        },
        BackendType::InMem => {
            make_session(InMemoryBackend::new())
        }
    };

    Box::into_raw(Box::new(session))
}


#[unsafe(no_mangle)]
pub extern "C" fn close(ptr: *mut LixSession,) -> bool {
    if ptr.is_null() {
        return false;
    }

    let mut session = unsafe {
        Box::from_raw(ptr)
    };

    let result = session.inner.close();
    return result;
}


#[unsafe(no_mangle)]
pub extern "C" fn get_active_branch(
    ptr: *mut LixSession,
    out_buf: *mut u8,
    buf_len: usize,
) -> usize {
    let Some(session) = (unsafe { ptr.as_mut() }) else {
        return 0;
    };

    return session.inner.get_active_branch(out_buf, buf_len);
}


#[unsafe(no_mangle)]
pub extern "C" fn create_branch(
    ptr: *mut LixSession,
    name_ptr: *const u8,
    name_len: usize,
    out_buf: *mut u8,
    buf_len: usize,
) -> usize {
    let Some(session) = (unsafe { ptr.as_mut() }) else {
        return 0;
    };

    return session.inner.create_branch(
        name_ptr,
        name_len,
        out_buf,
        buf_len,
    );
}

#[unsafe(no_mangle)]
pub extern "C" fn switch_branch(
    ptr: *mut LixSession,
    branch_id_ptr: *const u8,
    branch_id_len: usize,
) -> bool {
    let Some(session) = (unsafe { ptr.as_mut() }) else {
        return false;
    };

    return session.inner.change_branch(branch_id_ptr, branch_id_len);
}
