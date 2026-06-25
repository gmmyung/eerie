extern crate alloc;

use alloc::vec::Vec;

use super::{
    base,
    error::RuntimeError,
    hal::{self, BufferElement, BufferView, Value},
    vm::{self, ToRef},
};

/// Runtime HAL driver selection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Driver {
    LocalSync,
    #[cfg(feature = "std")]
    LocalTask,
    #[cfg(all(feature = "std", target_os = "macos"))]
    Metal,
    #[cfg(all(feature = "std", feature = "cuda"))]
    Cuda,
}

impl Driver {
    const fn name(self) -> &'static str {
        match self {
            Driver::LocalSync => "local-sync",
            #[cfg(feature = "std")]
            Driver::LocalTask => "local-task",
            #[cfg(all(feature = "std", target_os = "macos"))]
            Driver::Metal => "metal",
            #[cfg(all(feature = "std", feature = "cuda"))]
            Driver::Cuda => "cuda",
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
    /// Creates a runtime using the selected HAL driver and its default device.
    pub fn new(driver: Driver) -> Result<Self, RuntimeError> {
        let instance = vm::Instance::global()?;
        let registry = hal::DriverRegistry::with_available_drivers()?;
        let driver = registry.create_driver(driver.name())?;
        let device = driver.create_default_device()?;
        Ok(Self {
            instance,
            _registry: registry,
            _driver: driver,
            device,
            _not_send_sync: base::not_send_sync(),
        })
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
