use crate::{
    resource::{Resource, StoredResource},
    service::ZkStackService,
    task::Task,
    wiring_layer::WiringError,
};

/// An interface to the service's resources provided to the tasks during initialization.
/// Provides the ability to fetch required resources, and also gives access to the Tokio runtime handle.
#[derive(Debug)]
pub struct ServiceContext<'a> {
    service: &'a mut ZkStackService,
}

impl<'a> ServiceContext<'a> {
    pub(super) fn new(service: &'a mut ZkStackService) -> Self {
        Self { service }
    }

    /// Provides access to the runtime used by the service.
    /// Can be used to spawn additional tasks within the same runtime.
    /// If some tasks stores the handle to spawn additional tasks, it is expected to do all the required
    /// cleanup.
    ///
    /// In most cases, however, it is recommended to use [`add_task`] method instead.
    pub fn runtime_handle(&self) -> &tokio::runtime::Handle {
        self.service.runtime.handle()
    }

    /// Adds a task to the service.
    /// Added tasks will be launched after the wiring process will be finished.
    pub fn add_task(&mut self, task: Box<dyn Task>) -> &mut Self {
        self.service.tasks.push(task);
        self
    }

    /// Attempts to retrieve the resource with the specified name.
    /// Internally the resources are stored as [`std::any::Any`], and this method does the downcasting
    /// on behalf of the caller.
    ///
    /// ## Panics
    ///
    /// Panics if the resource with the specified name exists, but is not of the requested type.
    pub async fn get_resource<T: Resource + Clone>(&mut self) -> Result<T, WiringError> {
        #[allow(clippy::borrowed_box)]
        let downcast_clone = |resource: &Box<dyn StoredResource>| {
            resource
                .downcast_ref::<T>()
                .unwrap_or_else(|| {
                    panic!(
                        "Resource {} is not of type {}",
                        T::resource_id(),
                        std::any::type_name::<T>()
                    )
                })
                .clone()
        };

        let name = T::resource_id();
        // Check whether the resource is already available.
        if let Some(resource) = self.service.resources.get(&name) {
            return Ok(downcast_clone(resource));
        }

        // No such resource.
        // The requester is allowed to decide whether this is an error or not.
        Err(WiringError::ResourceLacking(T::resource_id()))
    }

    /// Attempts to retrieve the resource with the specified name.
    /// If the resource is not available, it is created using the provided closure.
    pub async fn get_resource_or_insert_with<T: Resource + Clone, F: FnOnce() -> T>(
        &mut self,
        f: F,
    ) -> T {
        if let Ok(resource) = self.get_resource::<T>().await {
            return resource;
        }

        // No such resource, insert a new one.
        let resource = f();
        self.service
            .resources
            .insert(T::resource_id(), Box::new(resource.clone()));
        resource
    }

    /// Attempts to retrieve the resource with the specified name.
    /// If the resource is not available, it is created using `T::default()`.
    pub async fn get_resource_or_default<T: Resource + Clone + Default>(&mut self) -> T {
        self.get_resource_or_insert_with(T::default).await
    }

    /// Adds a resource to the service.
    /// If the resource with the same name is already provided, the method will return an error.
    pub fn insert_resource<T: Resource>(&mut self, resource: T) -> Result<(), WiringError> {
        let name = T::resource_id();
        if self.service.resources.contains_key(&name) {
            return Err(WiringError::ResourceAlreadyProvided(name));
        }
        self.service.resources.insert(name, Box::new(resource));
        Ok(())
    }
}
