use std::{slice, str};
use tokio::runtime::{Builder, Runtime};
use lix_sdk::{
    Backend, CreateBranchOptions, ExecuteResult, InMemoryBackend, Lix, SqliteBackend, SwitchBranchOptions, Value, open_lix_with_backend
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

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct CStringView {
    pub offset: u32,
    pub len: u32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct CRowView {
    pub cells_offset: u32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct CQueryResultLayout {
    pub columns_offset: u32,
    pub rows_offset: u32,
    pub column_count: u32,
    pub row_count: u32,
    pub rows_affected: u64,
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

    fn execute(&mut self, sql_ptr: *const u8, sql_len: usize, out_buf: *mut u8, buf_len: usize) -> usize;
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

    fn execute(&mut self, sql_ptr: *const u8, sql_len: usize, out_buf: *mut u8, buf_len: usize) -> usize {
        let Ok(sql) = (unsafe {
            str::from_utf8(
                slice::from_raw_parts(sql_ptr, sql_len),
            )
        }) else { return 0; };

        let results: ExecuteResult = self.runtime.block_on(
            self.engine.execute(sql, &[])
        ).unwrap();

        let safe_out_buffer = unsafe { slice::from_raw_parts_mut(out_buf, buf_len) };
        let total_bytes_written = serialize_to_buffer(&results, safe_out_buffer);

        return total_bytes_written;
    }
}

//Duplicated from cli>output
fn value_to_text(value: &Value) -> String {
    match value {
        Value::Null => "null".to_string(),
        Value::Boolean(v) => v.to_string(),
        Value::Integer(v) => v.to_string(),
        Value::Real(v) => v.to_string(),
        Value::Text(v) => v.clone(),
        Value::Json(v) => v.to_string(),
        Value::Blob(bytes) => bytes_to_hex(bytes),
    }
}

fn bytes_to_hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2 + 2);
    out.push_str("0x");
    for byte in bytes {
        out.push(hex_digit(byte >> 4));
        out.push(hex_digit(byte & 0x0f));
    }
    out
}

fn hex_digit(value: u8) -> char {
    match value {
        0..=9 => (b'0' + value) as char,
        10..=15 => (b'a' + (value - 10)) as char,
        _ => '0',
    }
}

//Needs fixed, has a bunch of corrupt data in it
pub fn serialize_to_buffer(
    results: &ExecuteResult,
    out_buf: &mut [u8],
) -> usize {
    use std::mem::size_of;

    let col_cnt = results.columns().len();
    let row_cnt = results.rows().len();

    let root_sz = size_of::<CQueryResultLayout>();
    let cols_sz = size_of::<CStringView>() * col_cnt;
    let rows_sz = size_of::<CRowView>() * row_cnt;
    let cells_sz = size_of::<CStringView>() * col_cnt * row_cnt;

    let root_offset = 0usize;
    let cols_offset = root_offset + root_sz;
    let rows_offset = cols_offset + cols_sz;
    let cells_offset = rows_offset + rows_sz;
    let string_pool_offset = cells_offset + cells_sz;

    assert!(
        out_buf.len() >= string_pool_offset,
        "Buffer too small for metadata"
    );

    let (root_meta, remaining) = out_buf.split_at_mut(root_sz);
    let (col_meta, remaining) = remaining.split_at_mut(cols_sz);
    let (row_meta, remaining) = remaining.split_at_mut(rows_sz);
    let (cell_meta, string_pool) = remaining.split_at_mut(cells_sz);

    let root_slice = unsafe {
        std::slice::from_raw_parts_mut(
            root_meta.as_mut_ptr() as *mut CQueryResultLayout,
            1,
        )
    };

    let col_slice = unsafe {
        std::slice::from_raw_parts_mut(
            col_meta.as_mut_ptr() as *mut CStringView,
            col_cnt,
        )
    };

    let row_slice = unsafe {
        std::slice::from_raw_parts_mut(
            row_meta.as_mut_ptr() as *mut CRowView,
            row_cnt,
        )
    };

    let cell_slice = unsafe {
        std::slice::from_raw_parts_mut(
            cell_meta.as_mut_ptr() as *mut CStringView,
            col_cnt * row_cnt,
        )
    };

    let mut string_cursor = 0usize;

    let write_string = |s: &str,
                        pool: &mut [u8],
                        cursor: &mut usize|
     -> CStringView {
        let start = *cursor;
        let end = start + s.len();

        assert!(
            end <= pool.len(),
            "Buffer too small for string pool"
        );

        pool[start..end].copy_from_slice(s.as_bytes());
        *cursor = end;

        CStringView {
            offset: (string_pool_offset + start) as u32,
            len: s.len() as u32,
        }
    };

    //
    // Column headers
    //
    for (i, name) in results.columns().iter().enumerate() {
        col_slice[i] =
            write_string(name, string_pool, &mut string_cursor);
    }

    //
    // Cells + row views
    //
    for (row_idx, row) in results.rows().iter().enumerate() {
        let first_cell_idx = row_idx * col_cnt;

        for (col_idx, value) in row.values().iter().enumerate() {
            let text = value_to_text(value);

            cell_slice[first_cell_idx + col_idx] =
                write_string(&text, string_pool, &mut string_cursor);
        }

        let row_cells_offset =
            cells_offset +
            first_cell_idx * size_of::<CStringView>();

        row_slice[row_idx] = CRowView {
            cells_offset: row_cells_offset as u32,
        };
    }

    //
    // Root header
    //
    root_slice[0] = CQueryResultLayout {
        columns_offset: cols_offset as u32,
        rows_offset: rows_offset as u32,
        column_count: col_cnt as u32,
        row_count: row_cnt as u32,
        rows_affected: results.rows_affected(),
    };

    string_pool_offset + string_cursor
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

#[unsafe(no_mangle)]
pub extern "C" fn execute(ptr: *mut LixSession, sql_ptr: *const u8, sql_len: usize,
    out_buf: *mut u8, buf_len: usize) -> usize {
    let Some(session) = (unsafe { ptr.as_mut() }) else {
        return 0;
    };
    return session.inner.execute(sql_ptr,sql_len, out_buf, buf_len);
}
