use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicI64, Ordering};
use ort::{session::Session, value::Tensor};

static SESSIONS: Mutex<Option<HashMap<i64, Session>>> = Mutex::new(None);
static NEXT_HANDLE: AtomicI64 = AtomicI64::new(1);

#[no_mangle]
pub extern "C" fn fard_onnx_ping() -> i64 { 42 }

#[no_mangle]
pub extern "C" fn fard_onnx_load(path_ptr: i64, path_len: i64) -> i64 {
    let path = unsafe {
        let bytes = std::slice::from_raw_parts(path_ptr as *const u8, path_len as usize);
        match std::str::from_utf8(bytes) {
            Ok(s) => s.to_string(),
            Err(_) => return -1,
        }
    };
    let session = match Session::builder()
        .and_then(|mut b| b.commit_from_file(&path))
    {
        Ok(s) => s,
        Err(e) => { eprintln!("fard_onnx_load: {e}"); return -1; }
    };
    let handle = NEXT_HANDLE.fetch_add(1, Ordering::SeqCst);
    let mut guard = SESSIONS.lock().unwrap();
    if guard.is_none() { *guard = Some(HashMap::new()); }
    guard.as_mut().unwrap().insert(handle, session);
    handle
}

#[no_mangle]
pub extern "C" fn fard_onnx_infer(
    handle: i64,
    input_ptr: i64, input_len: i64,
    output_ptr: i64, output_len: i64,
) -> i64 {
    let input_vec: Vec<f32> = unsafe {
        std::slice::from_raw_parts(input_ptr as *const f32, input_len as usize).to_vec()
    };
    let output_slice = unsafe {
        std::slice::from_raw_parts_mut(output_ptr as *mut f32, output_len as usize)
    };

    let shape = [1usize, input_len as usize];
    let tensor: Tensor<f32> = match Tensor::from_array((shape, input_vec.into_boxed_slice())) {
        Ok(t) => t,
        Err(e) => { eprintln!("tensor create: {e}"); return -1; }
    };

    let mut guard = SESSIONS.lock().unwrap();
    let session = match guard.as_mut().and_then(|m| m.get_mut(&handle)) {
        Some(s) => s,
        None => return -1,
    };

    let in_name = session.inputs()[0].name().to_string();
    let out_name = session.outputs()[0].name().to_string();

    let outputs = match session.run(ort::inputs![in_name => tensor]) {
        Ok(o) => o,
        Err(e) => { eprintln!("session run: {e}"); return -1; }
    };

    if let Some(v) = outputs.get(&out_name) {
        // try_extract_tensor returns (&Shape, &[T])
        if let Ok((_shape, data)) = v.try_extract_tensor::<f32>() {
            let n = data.len().min(output_len as usize);
            output_slice[..n].copy_from_slice(&data[..n]);
            return 0;
        }
    }
    -1
}

#[no_mangle]
pub extern "C" fn fard_onnx_free(handle: i64) -> i64 {
    let mut guard = SESSIONS.lock().unwrap();
    match guard.as_mut().and_then(|m| m.remove(&handle)) {
        Some(_) => 0,
        None => -1,
    }
}
