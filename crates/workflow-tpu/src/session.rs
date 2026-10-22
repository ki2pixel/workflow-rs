use std::ffi::{c_char, c_int, c_void, CStr, CString};
use std::os::unix::ffi::OsStrExt;
use std::path::Path;
use std::ptr;

use crate::error::TpuError;
use crate::ffi::*;
use crate::EDGETPU_SIGNATURE;

const RTLD_NOW: c_int = 2;
const RTLD_LOCAL: c_int = 0;

#[link(name = "dl")]
extern "C" {
    fn dlopen(filename: *const c_char, flag: c_int) -> *mut c_void;
    fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *mut c_void;
    fn dlerror() -> *mut c_char;
    fn dlclose(handle: *mut c_void) -> c_int;
}

/// Spécification d'un tenseur d'entrée ou de sortie.
#[derive(Debug, Clone)]
pub struct TensorSpec {
    pub name: String,
    pub dims: Vec<i32>,
    pub ty: TfLiteType,
    pub bytes: usize,
    pub scale: f32,
    pub zero_point: i32,
}

struct Library {
    handle: *mut c_void,
}

impl Library {
    fn open(path: &Path) -> Result<Self, TpuError> {
        let cpath = CString::new(path.as_os_str().as_bytes())?;
        let handle = unsafe { dlopen(cpath.as_ptr(), RTLD_NOW | RTLD_LOCAL) };
        if handle.is_null() {
            return Err(TpuError::DlOpen {
                path: path.to_path_buf(),
                message: last_dl_error(),
            });
        }
        Ok(Self { handle })
    }

    fn symbol<T: Copy>(&self, name: &str) -> Result<T, TpuError> {
        assert_eq!(
            std::mem::size_of::<T>(),
            std::mem::size_of::<*mut c_void>(),
            "un symbole doit tenir dans un pointeur"
        );
        let cname = CString::new(name)?;
        unsafe { dlerror() };
        let raw = unsafe { dlsym(self.handle, cname.as_ptr()) };
        if raw.is_null() {
            return Err(TpuError::Symbol {
                name: name.to_string(),
                message: last_dl_error(),
            });
        }
        Ok(unsafe { std::mem::transmute_copy::<*mut c_void, T>(&raw) })
    }
}

impl Drop for Library {
    fn drop(&mut self) {
        unsafe { dlclose(self.handle) };
    }
}

fn last_dl_error() -> String {
    let ptr = unsafe { dlerror() };
    if ptr.is_null() {
        "erreur inconnue".to_string()
    } else {
        let message = unsafe { CStr::from_ptr(ptr) };
        message.to_string_lossy().into_owned()
    }
}

struct TfliteApi {
    model_create_from_file: FnModelCreateFromFile,
    model_delete: FnModelDelete,
    options_create: FnOptionsCreate,
    options_delete: FnOptionsDelete,
    options_set_num_threads: FnOptionsSetNumThreads,
    options_add_delegate: FnOptionsAddDelegate,
    interpreter_create: FnInterpreterCreate,
    interpreter_delete: FnInterpreterDelete,
    allocate_tensors: FnInterpreterAllocateTensors,
    invoke: FnInterpreterInvoke,
    input_count: FnInterpreterGetInputCount,
    output_count: FnInterpreterGetOutputCount,
    input_tensor: FnInterpreterGetInputTensor,
    output_tensor: FnInterpreterGetOutputTensor,
    tensor_type: FnTensorType,
    tensor_num_dims: FnTensorNumDims,
    tensor_dim: FnTensorDim,
    tensor_byte_size: FnTensorByteSize,
    tensor_name: FnTensorName,
    tensor_quantization: FnTensorQuantizationParams,
    tensor_copy_from: FnTensorCopyFromBuffer,
    tensor_copy_to: FnTensorCopyToBuffer,
    _lib: Library,
}

impl TfliteApi {
    fn load(path: &Path) -> Result<Self, TpuError> {
        let lib = Library::open(path)?;
        Ok(Self {
            model_create_from_file: lib.symbol("TfLiteModelCreateFromFile")?,
            model_delete: lib.symbol("TfLiteModelDelete")?,
            options_create: lib.symbol("TfLiteInterpreterOptionsCreate")?,
            options_delete: lib.symbol("TfLiteInterpreterOptionsDelete")?,
            options_set_num_threads: lib.symbol("TfLiteInterpreterOptionsSetNumThreads")?,
            options_add_delegate: lib.symbol("TfLiteInterpreterOptionsAddDelegate")?,
            interpreter_create: lib.symbol("TfLiteInterpreterCreate")?,
            interpreter_delete: lib.symbol("TfLiteInterpreterDelete")?,
            allocate_tensors: lib.symbol("TfLiteInterpreterAllocateTensors")?,
            invoke: lib.symbol("TfLiteInterpreterInvoke")?,
            input_count: lib.symbol("TfLiteInterpreterGetInputTensorCount")?,
            output_count: lib.symbol("TfLiteInterpreterGetOutputTensorCount")?,
            input_tensor: lib.symbol("TfLiteInterpreterGetInputTensor")?,
            output_tensor: lib.symbol("TfLiteInterpreterGetOutputTensor")?,
            tensor_type: lib.symbol("TfLiteTensorType")?,
            tensor_num_dims: lib.symbol("TfLiteTensorNumDims")?,
            tensor_dim: lib.symbol("TfLiteTensorDim")?,
            tensor_byte_size: lib.symbol("TfLiteTensorByteSize")?,
            tensor_name: lib.symbol("TfLiteTensorName")?,
            tensor_quantization: lib.symbol("TfLiteTensorQuantizationParams")?,
            tensor_copy_from: lib.symbol("TfLiteTensorCopyFromBuffer")?,
            tensor_copy_to: lib.symbol("TfLiteTensorCopyToBuffer")?,
            _lib: lib,
        })
    }
}

struct EdgetpuApi {
    list_devices: FnEdgetpuListDevices,
    free_devices: FnEdgetpuFreeDevices,
    create_delegate: FnEdgetpuCreateDelegate,
    free_delegate: FnEdgetpuFreeDelegate,
    version: FnEdgetpuVersion,
    _lib: Library,
}

impl EdgetpuApi {
    fn load(path: &Path) -> Result<Self, TpuError> {
        let lib = Library::open(path)?;
        Ok(Self {
            list_devices: lib.symbol("edgetpu_list_devices")?,
            free_devices: lib.symbol("edgetpu_free_devices")?,
            create_delegate: lib.symbol("edgetpu_create_delegate")?,
            free_delegate: lib.symbol("edgetpu_free_delegate")?,
            version: lib.symbol("edgetpu_version")?,
            _lib: lib,
        })
    }
}

unsafe fn read_spec(api: &TfliteApi, tensor: *const TfLiteTensor) -> TensorSpec {
    let num_dims = unsafe { (api.tensor_num_dims)(tensor) };
    let mut dims = Vec::with_capacity(num_dims.max(0) as usize);
    for index in 0..num_dims {
        dims.push(unsafe { (api.tensor_dim)(tensor, index) });
    }

    let name_ptr = unsafe { (api.tensor_name)(tensor) };
    let name = if name_ptr.is_null() {
        String::new()
    } else {
        let value = unsafe { CStr::from_ptr(name_ptr) };
        value.to_string_lossy().into_owned()
    };

    let quantization = unsafe { (api.tensor_quantization)(tensor) };

    TensorSpec {
        name,
        dims,
        ty: unsafe { (api.tensor_type)(tensor) },
        bytes: unsafe { (api.tensor_byte_size)(tensor) },
        scale: quantization.scale,
        zero_point: quantization.zero_point,
    }
}

unsafe fn release(
    tflite: &TfliteApi,
    edgetpu: Option<&EdgetpuApi>,
    model: *mut TfLiteModel,
    options: *mut TfLiteInterpreterOptions,
    interpreter: *mut TfLiteInterpreter,
    delegate: *mut TfLiteDelegate,
) {
    if !interpreter.is_null() {
        unsafe { (tflite.interpreter_delete)(interpreter) };
    }
    if !options.is_null() {
        unsafe { (tflite.options_delete)(options) };
    }
    if !model.is_null() {
        unsafe { (tflite.model_delete)(model) };
    }
    if !delegate.is_null() {
        if let Some(edgetpu) = edgetpu {
            unsafe { (edgetpu.free_delegate)(delegate) };
        }
    }
}

/// Vérifie la présence de la signature `edgetpu-custom-op` dans le modèle.
pub fn verify_edgetpu_model(path: &Path) -> Result<(), TpuError> {
    let data = std::fs::read(path)?;
    if data
        .windows(EDGETPU_SIGNATURE.len())
        .any(|window| window == EDGETPU_SIGNATURE)
    {
        Ok(())
    } else {
        Err(TpuError::ModelNotCompiled {
            path: path.to_path_buf(),
        })
    }
}

/// Session TFLite liée à un delegate Edge TPU.
///
/// Le cycle de vie est volontairement monolithique : les pointeurs sont libérés
/// dans l'ordre attendu par les deux bibliothèques C, sous peine d'utilisation
/// après libération.
pub struct Session {
    tflite: TfliteApi,
    edgetpu: Option<EdgetpuApi>,
    model: *mut TfLiteModel,
    options: *mut TfLiteInterpreterOptions,
    interpreter: *mut TfLiteInterpreter,
    delegate: *mut TfLiteDelegate,
    inputs: Vec<*mut TfLiteTensor>,
    outputs: Vec<*const TfLiteTensor>,
    input_specs: Vec<TensorSpec>,
    output_specs: Vec<TensorSpec>,
    output_buffer: Vec<u8>,
}

impl Session {
    /// Ouvre un modèle compilé pour l'Edge TPU avec un delegate Coral neuf.
    pub fn open(
        model_path: &Path,
        tflite_path: &Path,
        edgetpu_path: &Path,
        num_threads: i32,
    ) -> Result<Self, TpuError> {
        verify_edgetpu_model(model_path)?;

        let edgetpu = EdgetpuApi::load(edgetpu_path)?;
        let delegate =
            unsafe { (edgetpu.create_delegate)(EDGETPU_APEX_PCI, ptr::null(), ptr::null(), 0) };
        if delegate.is_null() {
            return Err(TpuError::DelegateUnavailable);
        }

        match Self::open_common(
            model_path,
            tflite_path,
            Some(&edgetpu),
            delegate,
            num_threads,
        ) {
            Ok(mut session) => {
                session.edgetpu = Some(edgetpu);
                Ok(session)
            }
            Err(error) => {
                unsafe { (edgetpu.free_delegate)(delegate) };
                Err(error)
            }
        }
    }

    /// Ouvre un modèle TFLite sur le backend CPU (XNNPACK/TFLite), sans Coral.
    ///
    /// Ce chemin est réservé aux tests et au dépannage ; le delegate TPU reste
    /// le backend nominal de production.
    pub fn open_cpu(
        model_path: &Path,
        tflite_path: &Path,
        num_threads: i32,
    ) -> Result<Self, TpuError> {
        Self::open_common(model_path, tflite_path, None, ptr::null_mut(), num_threads)
    }

    fn open_common(
        model_path: &Path,
        tflite_path: &Path,
        edgetpu: Option<&EdgetpuApi>,
        delegate: *mut TfLiteDelegate,
        num_threads: i32,
    ) -> Result<Self, TpuError> {
        let tflite = TfliteApi::load(tflite_path)?;

        let cpath = CString::new(model_path.as_os_str().as_bytes())?;
        let model = unsafe { (tflite.model_create_from_file)(cpath.as_ptr()) };
        if model.is_null() {
            return Err(TpuError::TfLite {
                context: "TfLiteModelCreateFromFile",
                status: -1,
            });
        }

        let options = unsafe { (tflite.options_create)() };
        if options.is_null() {
            unsafe {
                (tflite.model_delete)(model);
            }
            return Err(TpuError::TfLite {
                context: "TfLiteInterpreterOptionsCreate",
                status: -1,
            });
        }

        unsafe {
            (tflite.options_set_num_threads)(options, num_threads);
            if !delegate.is_null() {
                (tflite.options_add_delegate)(options, delegate);
            }
        }

        let interpreter = unsafe { (tflite.interpreter_create)(model, options) };
        if interpreter.is_null() {
            unsafe {
                release(
                    &tflite,
                    edgetpu,
                    model,
                    options,
                    ptr::null_mut(),
                    ptr::null_mut(),
                )
            };
            return Err(TpuError::TfLite {
                context: "TfLiteInterpreterCreate",
                status: -1,
            });
        }

        let status = unsafe { (tflite.allocate_tensors)(interpreter) };
        if status != TFLITE_OK {
            unsafe {
                release(
                    &tflite,
                    edgetpu,
                    model,
                    options,
                    interpreter,
                    ptr::null_mut(),
                )
            };
            return Err(TpuError::TfLite {
                context: "TfLiteInterpreterAllocateTensors",
                status,
            });
        }

        let input_count = unsafe { (tflite.input_count)(interpreter) };
        let output_count = unsafe { (tflite.output_count)(interpreter) };
        if input_count <= 0 || output_count <= 0 {
            unsafe {
                release(
                    &tflite,
                    edgetpu,
                    model,
                    options,
                    interpreter,
                    ptr::null_mut(),
                )
            };
            return Err(TpuError::TfLite {
                context: "comptage des tenseurs",
                status: -1,
            });
        }

        let mut inputs = Vec::with_capacity(input_count as usize);
        let mut input_specs = Vec::with_capacity(input_count as usize);
        for index in 0..input_count {
            let tensor = unsafe { (tflite.input_tensor)(interpreter, index) };
            if tensor.is_null() {
                unsafe {
                    release(
                        &tflite,
                        edgetpu,
                        model,
                        options,
                        interpreter,
                        ptr::null_mut(),
                    )
                };
                return Err(TpuError::NullTensor {
                    context: "TfLiteInterpreterGetInputTensor",
                    index,
                });
            }
            input_specs.push(unsafe { read_spec(&tflite, tensor) });
            inputs.push(tensor);
        }

        let mut outputs = Vec::with_capacity(output_count as usize);
        let mut output_specs = Vec::with_capacity(output_count as usize);
        for index in 0..output_count {
            let tensor = unsafe { (tflite.output_tensor)(interpreter, index) };
            if tensor.is_null() {
                unsafe {
                    release(
                        &tflite,
                        edgetpu,
                        model,
                        options,
                        interpreter,
                        ptr::null_mut(),
                    )
                };
                return Err(TpuError::NullTensor {
                    context: "TfLiteInterpreterGetOutputTensor",
                    index,
                });
            }
            output_specs.push(unsafe { read_spec(&tflite, tensor) });
            outputs.push(tensor);
        }

        let total_output = output_specs.iter().map(|spec| spec.bytes).sum();

        Ok(Self {
            tflite,
            edgetpu: None,
            model,
            options,
            interpreter,
            delegate,
            inputs,
            outputs,
            input_specs,
            output_specs,
            output_buffer: vec![0u8; total_output],
        })
    }

    pub fn input_specs(&self) -> &[TensorSpec] {
        &self.input_specs
    }

    pub fn output_specs(&self) -> &[TensorSpec] {
        &self.output_specs
    }

    pub fn edgetpu_version(&self) -> Option<String> {
        let edgetpu = self.edgetpu.as_ref()?;
        let ptr = unsafe { (edgetpu.version)() };
        if ptr.is_null() {
            None
        } else {
            let value = unsafe { CStr::from_ptr(ptr) };
            Some(value.to_string_lossy().into_owned())
        }
    }

    pub fn device_paths(&self) -> Vec<String> {
        let Some(edgetpu) = self.edgetpu.as_ref() else {
            return Vec::new();
        };
        let mut count = 0usize;
        let devices = unsafe { (edgetpu.list_devices)(&mut count) };
        if devices.is_null() {
            return Vec::new();
        }

        let mut paths = Vec::with_capacity(count);
        for index in 0..count {
            let device = unsafe { &*devices.add(index) };
            if !device.path.is_null() {
                let path = unsafe { CStr::from_ptr(device.path) };
                paths.push(path.to_string_lossy().into_owned());
            }
        }

        unsafe { (edgetpu.free_devices)(devices) };
        paths
    }

    /// Écrit les entrées, invoque, puis copie les sorties dans un tampon contigu.
    pub fn invoke(&mut self, inputs: &[&[u8]]) -> Result<(), TpuError> {
        if inputs.len() != self.inputs.len() {
            return Err(TpuError::WrongInputCount {
                expected: self.inputs.len(),
                got: inputs.len(),
            });
        }

        for (index, (tensor, data)) in self.inputs.iter().zip(inputs.iter()).enumerate() {
            let expected = self.input_specs[index].bytes;
            if data.len() != expected {
                return Err(TpuError::WrongInputSize {
                    index,
                    expected,
                    got: data.len(),
                });
            }
            let status = unsafe {
                (self.tflite.tensor_copy_from)(*tensor, data.as_ptr() as *const c_void, data.len())
            };
            if status != TFLITE_OK {
                return Err(TpuError::TfLite {
                    context: "TfLiteTensorCopyFromBuffer",
                    status,
                });
            }
        }

        let status = unsafe { (self.tflite.invoke)(self.interpreter) };
        if status != TFLITE_OK {
            return Err(TpuError::TfLite {
                context: "TfLiteInterpreterInvoke",
                status,
            });
        }

        let outputs = self.outputs.clone();
        let mut offset = 0usize;
        for (index, tensor) in outputs.iter().enumerate() {
            let bytes = self.output_specs[index].bytes;
            let destination = &mut self.output_buffer[offset..offset + bytes];
            let status = unsafe {
                (self.tflite.tensor_copy_to)(
                    *tensor,
                    destination.as_mut_ptr() as *mut c_void,
                    bytes,
                )
            };
            if status != TFLITE_OK {
                return Err(TpuError::TfLite {
                    context: "TfLiteTensorCopyToBuffer",
                    status,
                });
            }
            offset += bytes;
        }

        Ok(())
    }

    pub fn output(&self) -> &[u8] {
        &self.output_buffer
    }

    pub fn output_segments(&self) -> Vec<(&TensorSpec, &[u8])> {
        let mut segments = Vec::with_capacity(self.output_specs.len());
        let mut offset = 0usize;
        for spec in &self.output_specs {
            segments.push((spec, &self.output_buffer[offset..offset + spec.bytes]));
            offset += spec.bytes;
        }
        segments
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        unsafe {
            release(
                &self.tflite,
                self.edgetpu.as_ref(),
                self.model,
                self.options,
                self.interpreter,
                self.delegate,
            )
        };
    }
}
