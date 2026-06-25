extern crate alloc;

use alloc::{borrow::Cow, string::String, vec::Vec};

use super::{
    base,
    error::RuntimeError,
    hal::{self, BufferElement, BufferView, Value},
    vm::{self, ToRef},
};

/// Runtime HAL driver name.
///
/// IREE drivers are selected by canonical names such as `local-sync`, `metal`,
/// or `vulkan`. Common names have constructors; external or downstream drivers
/// can be passed with `Driver::custom`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Driver {
    name: Cow<'static, str>,
}

impl Driver {
    fn from_static(name: &'static str) -> Self {
        Self {
            name: Cow::Borrowed(name),
        }
    }

    pub fn local_sync() -> Self {
        Self::from_static("local-sync")
    }

    pub fn local_task() -> Self {
        Self::from_static("local-task")
    }

    pub fn metal() -> Self {
        Self::from_static("metal")
    }

    pub fn vulkan() -> Self {
        Self::from_static("vulkan")
    }

    pub fn cuda() -> Self {
        Self::from_static("cuda")
    }

    pub fn custom(name: impl Into<String>) -> Self {
        Self {
            name: Cow::Owned(name.into()),
        }
    }

    pub fn as_str(&self) -> &str {
        &self.name
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum DeviceSelector {
    Default,
    Ordinal(usize),
    Path(String),
}

/// Runtime device selection.
///
/// `Runtime::new` accepts a single `DeviceSpec`, keeping runtime creation
/// simple while still allowing multi-device drivers to select a specific device.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeviceSpec {
    driver: Driver,
    selector: DeviceSelector,
}

impl DeviceSpec {
    pub fn new(driver: Driver) -> Self {
        Self {
            driver,
            selector: DeviceSelector::Default,
        }
    }

    pub fn local_sync() -> Self {
        Self::new(Driver::local_sync())
    }

    pub fn local_task() -> Self {
        Self::new(Driver::local_task())
    }

    pub fn metal() -> Self {
        Self::new(Driver::metal())
    }

    pub fn vulkan() -> Self {
        Self::new(Driver::vulkan())
    }

    pub fn cuda() -> Self {
        Self::new(Driver::cuda())
    }

    pub fn custom(driver: impl Into<String>) -> Self {
        Self::new(Driver::custom(driver))
    }

    pub fn ordinal(mut self, ordinal: usize) -> Self {
        self.selector = DeviceSelector::Ordinal(ordinal);
        self
    }

    pub fn path(mut self, path: impl Into<String>) -> Self {
        self.selector = DeviceSelector::Path(path.into());
        self
    }

    pub fn driver(&self) -> &Driver {
        &self.driver
    }
}

/// One device reported by a HAL driver.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeviceInfo {
    driver: Driver,
    ordinal: usize,
    path: String,
    name: String,
}

impl DeviceInfo {
    fn from_hal(driver: Driver, info: hal::DeviceInfo) -> Self {
        Self {
            driver,
            ordinal: info.ordinal,
            path: info.path,
            name: info.name,
        }
    }

    pub fn driver(&self) -> &Driver {
        &self.driver
    }

    pub fn ordinal(&self) -> usize {
        self.ordinal
    }

    pub fn path(&self) -> &str {
        &self.path
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn spec(&self) -> DeviceSpec {
        let spec = DeviceSpec::new(self.driver.clone());
        if self.path.is_empty() {
            spec.ordinal(self.ordinal)
        } else {
            spec.path(self.path.clone())
        }
    }
}

/// A configured IREE runtime with one HAL device.
///
/// `Runtime` owns the low-level driver objects needed to keep the selected
/// device alive. Load VMFB modules into `Program`s and reuse them for calls.
pub struct Runtime {
    instance: vm::Instance,
    _registry: hal::DriverRegistry,
    _driver: hal::Driver,
    device: hal::Device,
    _not_send_sync: base::NotSendSync,
}

impl Runtime {
    /// Creates a runtime using the selected HAL device.
    pub fn new(spec: DeviceSpec) -> Result<Self, RuntimeError> {
        let instance = vm::Instance::global()?;
        let registry = hal::DriverRegistry::with_available_drivers()?;
        let driver = registry.create_driver(spec.driver.as_str())?;
        let device = match &spec.selector {
            DeviceSelector::Default => driver.create_default_device()?,
            DeviceSelector::Ordinal(ordinal) => driver.create_device_by_ordinal(*ordinal)?,
            DeviceSelector::Path(path) => {
                driver.create_device_by_path(spec.driver.as_str(), path)?
            }
        };
        Ok(Self {
            instance,
            _registry: registry,
            _driver: driver,
            device,
            _not_send_sync: base::not_send_sync(),
        })
    }

    /// Queries available devices for a HAL driver.
    pub fn available_devices(driver: Driver) -> Result<Vec<DeviceInfo>, RuntimeError> {
        let registry = hal::DriverRegistry::with_available_drivers()?;
        let hal_driver = registry.create_driver(driver.as_str())?;
        hal_driver
            .available_devices()?
            .into_iter()
            .map(|info| Ok(DeviceInfo::from_hal(driver.clone(), info)))
            .collect()
    }

    /// Loads a VMFB archive into an executable program.
    pub fn load_vmfb(&self, bytes: &[u8]) -> Result<Program, RuntimeError> {
        Program::load(self, bytes)
    }

    /// Allocates a dense row-major typed buffer view on this runtime's device.
    pub fn buffer_view<T: BufferElement>(
        &self,
        shape: &[usize],
        data: &[T],
    ) -> Result<BufferView<T>, RuntimeError> {
        BufferView::from_host(&self.device, shape, data)
    }
}

/// A loaded VMFB program with its own VM context.
pub struct Program {
    instance: vm::Instance,
    device: hal::Device,
    context: vm::Context,
    _hal_module: vm::Module,
    _bytecode_module: vm::Module,
    _not_send_sync: base::NotSendSync,
}

impl Program {
    fn load(runtime: &Runtime, bytes: &[u8]) -> Result<Self, RuntimeError> {
        let hal_module = vm::Module::hal(&runtime.instance, &runtime.device)?;
        let bytecode_module = vm::Module::bytecode(&runtime.instance, bytes)?;
        let context =
            vm::Context::with_modules(&runtime.instance, &[&hal_module, &bytecode_module])?;
        Ok(Self {
            instance: runtime.instance.clone(),
            device: runtime.device.clone(),
            context,
            _hal_module: hal_module,
            _bytecode_module: bytecode_module,
            _not_send_sync: base::not_send_sync(),
        })
    }

    pub fn function(&self, name: &str) -> Result<Function, RuntimeError> {
        Ok(Function {
            inner: self.context.resolve_function(name)?,
            instance: self.instance.clone(),
            device: self.device.clone(),
            _not_send_sync: base::not_send_sync(),
        })
    }
}

fn push_value(
    list: &mut vm::List<vm::Undefined>,
    instance: &vm::Instance,
    value: &Value,
) -> Result<(), RuntimeError> {
    match value {
        Value::Bool(buffer) => list.push_ref(&buffer.to_ref(instance)?)?,
        Value::U8(buffer) => list.push_ref(&buffer.to_ref(instance)?)?,
        Value::U16(buffer) => list.push_ref(&buffer.to_ref(instance)?)?,
        Value::U32(buffer) => list.push_ref(&buffer.to_ref(instance)?)?,
        Value::U64(buffer) => list.push_ref(&buffer.to_ref(instance)?)?,
        Value::I8(buffer) => list.push_ref(&buffer.to_ref(instance)?)?,
        Value::I16(buffer) => list.push_ref(&buffer.to_ref(instance)?)?,
        Value::I32(buffer) => list.push_ref(&buffer.to_ref(instance)?)?,
        Value::I64(buffer) => list.push_ref(&buffer.to_ref(instance)?)?,
        #[cfg(feature = "half")]
        Value::F16(buffer) => list.push_ref(&buffer.to_ref(instance)?)?,
        Value::F32(buffer) => list.push_ref(&buffer.to_ref(instance)?)?,
        Value::F64(buffer) => list.push_ref(&buffer.to_ref(instance)?)?,
        #[cfg(feature = "half")]
        Value::Bf16(buffer) => list.push_ref(&buffer.to_ref(instance)?)?,
    }
    Ok(())
}

/// A resolved program function.
pub struct Function {
    inner: vm::Function,
    instance: vm::Instance,
    device: hal::Device,
    _not_send_sync: base::NotSendSync,
}

impl Function {
    /// Invokes with HAL buffer-view inputs and returns dynamically typed outputs.
    pub fn invoke<I, V>(&self, inputs: I) -> Result<Vec<Value>, RuntimeError>
    where
        I: IntoIterator<Item = V>,
        V: Into<Value>,
    {
        let inputs = inputs.into_iter().map(Into::into).collect::<Vec<_>>();
        let mut input_list = vm::List::<vm::Undefined>::new(inputs.len(), &self.instance)?;
        for input in &inputs {
            push_value(&mut input_list, &self.instance, input)?;
        }

        let output_count = self.inner.result_count()?;
        let mut output_list = vm::List::<vm::Undefined>::new(output_count, &self.instance)?;
        self.inner.invoke(&input_list, &mut output_list)?;

        let mut outputs = Vec::with_capacity(output_count);
        for index in 0..output_count {
            outputs.push(output_list.get_buffer_view_value(index, &self.device)?);
        }
        Ok(outputs)
    }
}
