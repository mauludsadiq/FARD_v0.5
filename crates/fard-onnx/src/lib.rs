use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicI64, Ordering};
use ort::{session::Session, value::Tensor};

static SESSIONS: std::sync::RwLock<Option<HashMap<i64, Session>>> = std::sync::RwLock::new(None);
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
    let mut guard = SESSIONS.write().unwrap();
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

    let mut guard = SESSIONS.write().unwrap();
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
    let mut guard = SESSIONS.write().unwrap();
    match guard.as_mut().and_then(|m| m.remove(&handle)) {
        Some(_) => 0,
        None => -1,
    }
}

/// JSON-based inference — takes input as JSON float array string, returns JSON float array.
/// All data passed as pointer+length pairs (i64), compatible with ffi.call_checked.
/// Input:  json_ptr/json_len -> "[0.1, 0.2, ...]" (25 floats)
/// Output: out_ptr/out_len  -> pre-allocated buffer for result JSON
/// Returns: number of bytes written, or -1 on error
#[no_mangle]
pub extern "C" fn fard_onnx_infer_json(
    handle: i64,
    json_ptr: i64, json_len: i64,
    out_ptr: i64, out_len: i64,
) -> i64 {
    // Parse input JSON
    let json_str = unsafe {
        let bytes = std::slice::from_raw_parts(json_ptr as *const u8, json_len as usize);
        match std::str::from_utf8(bytes) {
            Ok(s) => s.to_string(),
            Err(_) => return -1,
        }
    };

    // Parse float array from JSON string like "[0.1,0.2,...]"
    let trimmed = json_str.trim().trim_start_matches('[').trim_end_matches(']');
    let input_vec: Vec<f32> = trimmed
        .split(',')
        .filter_map(|s| s.trim().parse::<f32>().ok())
        .collect();

    if input_vec.is_empty() { return -1; }
    let input_len = input_vec.len();

    let shape = [1usize, input_len];
    let tensor: Tensor<f32> = match Tensor::from_array((shape, input_vec.into_boxed_slice())) {
        Ok(t) => t,
        Err(_) => return -1,
    };

    let mut guard = SESSIONS.write().unwrap();
    let session = match guard.as_mut().and_then(|m| m.get_mut(&handle)) {
        Some(s) => s,
        None => return -1,
    };

    let in_name = session.inputs()[0].name().to_string();
    let out_name = session.outputs()[0].name().to_string();

    let outputs = match session.run(ort::inputs![in_name => tensor]) {
        Ok(o) => o,
        Err(e) => { eprintln!("fard_onnx_infer_json run: {e}"); return -1; }
    };

    // Serialize output to JSON
    let result_json = if let Some(v) = outputs.get(&out_name) {
        if let Ok((_shape, data)) = v.try_extract_tensor::<f32>() {
            let parts: Vec<String> = data.iter().map(|f| format!("{:.6}", f)).collect();
            format!("[{}]", parts.join(","))
        } else { return -1; }
    } else { return -1; };

    // Write to output buffer
    let out_slice = unsafe {
        std::slice::from_raw_parts_mut(out_ptr as *mut u8, out_len as usize)
    };
    let bytes = result_json.as_bytes();
    let n = bytes.len().min(out_len as usize);
    out_slice[..n].copy_from_slice(&bytes[..n]);
    n as i64
}

use std::sync::RwLock as ResultRwLock;
static LAST_RESULT: ResultRwLock<String> = ResultRwLock::new(String::new());
static LAST_RESULT_LEN: std::sync::atomic::AtomicI64 = std::sync::atomic::AtomicI64::new(0);

/// Simple inference — stores result in global, retrieve with fard_onnx_get_result_ptr/len
/// Returns number of output floats, or -1 on error
#[no_mangle]
pub extern "C" fn fard_onnx_infer_store(
    handle: i64,
    json_ptr: i64, json_len: i64,
) -> i64 {
    let json_str = unsafe {
        let bytes = std::slice::from_raw_parts(json_ptr as *const u8, json_len as usize);
        match std::str::from_utf8(bytes) {
            Ok(s) => s.to_string(),
            Err(_) => return -1,
        }
    };
    let trimmed = json_str.trim().trim_start_matches('[').trim_end_matches(']');
    let input_vec: Vec<f32> = trimmed
        .split(',')
        .filter_map(|s| s.trim().parse::<f32>().ok())
        .collect();
    if input_vec.is_empty() { return -1; }
    let input_len = input_vec.len();
    let shape = [1usize, input_len];
    let tensor: Tensor<f32> = match Tensor::from_array((shape, input_vec.into_boxed_slice())) {
        Ok(t) => t,
        Err(_) => return -1,
    };
    let mut guard = SESSIONS.write().unwrap();
    let session = match guard.as_mut().and_then(|m| m.get_mut(&handle)) {
        Some(s) => s,
        None => return -1,
    };
    let in_name = session.inputs()[0].name().to_string();
    let out_name = session.outputs()[0].name().to_string();
    let outputs = match session.run(ort::inputs![in_name => tensor]) {
        Ok(o) => o,
        Err(e) => { eprintln!("infer_store: {e}"); return -1; }
    };
    if let Some(v) = outputs.get(&out_name) {
        if let Ok((_shape, data)) = v.try_extract_tensor::<f32>() {
            let parts: Vec<String> = data.iter().map(|f| format!("{:.6}", f)).collect();
            let json = format!("[{}]", parts.join(","));
            let n = data.len() as i64;
            *LAST_RESULT.write().unwrap() = json;
            LAST_RESULT_LEN.store(n, Ordering::SeqCst);
            return n;
        }
    }
    -1
}

/// Get pointer to last stored result string
#[no_mangle]
pub extern "C" fn fard_onnx_result_ptr() -> i64 {
    LAST_RESULT.read().unwrap().as_ptr() as i64
}

/// Get length of last stored result string
#[no_mangle]
pub extern "C" fn fard_onnx_result_len() -> i64 {
    LAST_RESULT.read().unwrap().len() as i64
}

/// Return the argmax of the last stored inference result.
/// Returns the index of the highest logit, or -1 if no result stored.
#[no_mangle]
pub extern "C" fn fard_onnx_result_argmax() -> i64 {
    let guard = LAST_RESULT.read().unwrap();
    let json = guard.as_str();
    if json.is_empty() { return -1; }
    let trimmed = json.trim().trim_start_matches('[').trim_end_matches(']');
    let vals: Vec<f32> = trimmed
        .split(',')
        .filter_map(|s| s.trim().parse::<f32>().ok())
        .collect();
    if vals.is_empty() { return -1; }
    vals.iter()
        .enumerate()
        .max_by(|a, b| a.1.partial_cmp(b.1).unwrap_or(std::cmp::Ordering::Equal))
        .map(|(i, _)| i as i64)
        .unwrap_or(-1)
}

/// Return the value at a specific index of the last stored result.
/// Returns the raw bits of the f32 as i64, or -1 on error.
#[no_mangle]
pub extern "C" fn fard_onnx_result_at(idx: i64) -> i64 {
    let guard = LAST_RESULT.read().unwrap();
    let json = guard.as_str();
    if json.is_empty() { return -1; }
    let trimmed = json.trim().trim_start_matches('[').trim_end_matches(']');
    let vals: Vec<f32> = trimmed
        .split(',')
        .filter_map(|s| s.trim().parse::<f32>().ok())
        .collect();
    if idx < 0 || idx as usize >= vals.len() { return -1; }
    // Return as fixed-point * 1000000 to preserve sign and magnitude as i64
    (vals[idx as usize] * 1_000_000.0) as i64
}

/// Return the number of elements in the last stored result.
#[no_mangle]
pub extern "C" fn fard_onnx_result_count() -> i64 {
    let guard = LAST_RESULT.read().unwrap();
    let json = guard.as_str();
    if json.is_empty() { return 0; }
    let trimmed = json.trim().trim_start_matches('[').trim_end_matches(']');
    trimmed.split(',').filter(|s| !s.trim().is_empty()).count() as i64
}
