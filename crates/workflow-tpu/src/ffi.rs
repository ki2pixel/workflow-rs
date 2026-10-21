//! Surface FFI minimale pour l'API C de TensorFlow Lite et le delegate Coral.
//!
//! Les types sont opaques : seule leur adresse est manipulée. Les signatures
//! correspondent à `tensorflow/lite/core/c/c_api.h` (2.16) et à
//! `tflite/public/edgetpu_c.h` de libedgetpu.

#![allow(non_camel_case_types)]

use std::ffi::{c_char, c_int};

pub type TfLiteStatus = c_int;
pub const TFLITE_OK: TfLiteStatus = 0;

/// Valeurs de `TfLiteType` dans `c_api_types.h`.
pub type TfLiteType = c_int;
pub const TFLITE_FLOAT32: TfLiteType = 1;
pub const TFLITE_INT32: TfLiteType = 2;
pub const TFLITE_UINT8: TfLiteType = 3;
pub const TFLITE_INT8: TfLiteType = 9;
pub const TFLITE_FLOAT16: TfLiteType = 10;

/// `enum edgetpu_device_type`.
pub type EdgetpuDeviceType = c_int;
pub const EDGETPU_APEX_PCI: EdgetpuDeviceType = 0;
pub const EDGETPU_APEX_USB: EdgetpuDeviceType = 1;

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct TfLiteQuantizationParams {
    pub scale: f32,
    pub zero_point: i32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct EdgetpuOption {
    pub name: *const c_char,
    pub value: *const c_char,
}

#[repr(C)]
pub struct EdgetpuDevice {
    pub device_type: EdgetpuDeviceType,
    pub path: *const c_char,
}

macro_rules! opaque {
    ($($name:ident),* $(,)?) => {
        $(
            #[repr(C)]
            pub struct $name {
                _private: [u8; 0],
            }
        )*
    };
}

opaque!(
    TfLiteModel,
    TfLiteInterpreterOptions,
    TfLiteInterpreter,
    TfLiteTensor,
    TfLiteDelegate,
);

pub type FnModelCreateFromFile = unsafe extern "C" fn(*const c_char) -> *mut TfLiteModel;
pub type FnModelDelete = unsafe extern "C" fn(*mut TfLiteModel);

pub type FnOptionsCreate = unsafe extern "C" fn() -> *mut TfLiteInterpreterOptions;
pub type FnOptionsDelete = unsafe extern "C" fn(*mut TfLiteInterpreterOptions);
pub type FnOptionsSetNumThreads = unsafe extern "C" fn(*mut TfLiteInterpreterOptions, i32);
pub type FnOptionsAddDelegate =
    unsafe extern "C" fn(*mut TfLiteInterpreterOptions, *mut TfLiteDelegate);

pub type FnInterpreterCreate = unsafe extern "C" fn(
    *const TfLiteModel,
    *const TfLiteInterpreterOptions,
) -> *mut TfLiteInterpreter;
pub type FnInterpreterDelete = unsafe extern "C" fn(*mut TfLiteInterpreter);
pub type FnInterpreterAllocateTensors =
    unsafe extern "C" fn(*mut TfLiteInterpreter) -> TfLiteStatus;
pub type FnInterpreterInvoke = unsafe extern "C" fn(*mut TfLiteInterpreter) -> TfLiteStatus;
pub type FnInterpreterGetInputCount = unsafe extern "C" fn(*const TfLiteInterpreter) -> i32;
pub type FnInterpreterGetOutputCount = unsafe extern "C" fn(*const TfLiteInterpreter) -> i32;
pub type FnInterpreterGetInputTensor =
    unsafe extern "C" fn(*const TfLiteInterpreter, i32) -> *mut TfLiteTensor;
pub type FnInterpreterGetOutputTensor =
    unsafe extern "C" fn(*const TfLiteInterpreter, i32) -> *const TfLiteTensor;

pub type FnTensorType = unsafe extern "C" fn(*const TfLiteTensor) -> TfLiteType;
pub type FnTensorNumDims = unsafe extern "C" fn(*const TfLiteTensor) -> i32;
pub type FnTensorDim = unsafe extern "C" fn(*const TfLiteTensor, i32) -> i32;
pub type FnTensorByteSize = unsafe extern "C" fn(*const TfLiteTensor) -> usize;
pub type FnTensorName = unsafe extern "C" fn(*const TfLiteTensor) -> *const c_char;
pub type FnTensorQuantizationParams =
    unsafe extern "C" fn(*const TfLiteTensor) -> TfLiteQuantizationParams;
pub type FnTensorCopyFromBuffer =
    unsafe extern "C" fn(*mut TfLiteTensor, *const std::ffi::c_void, usize) -> TfLiteStatus;
pub type FnTensorCopyToBuffer =
    unsafe extern "C" fn(*const TfLiteTensor, *mut std::ffi::c_void, usize) -> TfLiteStatus;

pub type FnEdgetpuListDevices = unsafe extern "C" fn(*mut usize) -> *mut EdgetpuDevice;
pub type FnEdgetpuFreeDevices = unsafe extern "C" fn(*mut EdgetpuDevice);
pub type FnEdgetpuCreateDelegate = unsafe extern "C" fn(
    EdgetpuDeviceType,
    *const c_char,
    *const EdgetpuOption,
    usize,
) -> *mut TfLiteDelegate;
pub type FnEdgetpuFreeDelegate = unsafe extern "C" fn(*mut TfLiteDelegate);
pub type FnEdgetpuVersion = unsafe extern "C" fn() -> *const c_char;
