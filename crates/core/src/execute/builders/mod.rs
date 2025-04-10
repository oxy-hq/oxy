use concurrency::DefaultControl;
use wrap::Wrap;

use crate::adapters::checkpoint::CheckpointManager;

pub mod cache;
pub mod chain;
pub mod checkpoint;
pub mod concurrency;
pub mod consistency;
pub mod export;
pub mod map;
pub mod react;
mod stack;
pub mod state;
pub mod utils;
mod wrap;

#[derive(Clone, Debug)]
pub struct Root;

impl<E> Wrap<E> for Root {
    type Wrapper = E;

    fn wrap(&self, inner: E) -> Self::Wrapper {
        inner
    }
}

#[derive(Clone, Debug)]
pub struct ExecutableBuilder<W> {
    wrapper: W,
}

impl Default for ExecutableBuilder<Root> {
    fn default() -> Self {
        Self::new()
    }
}

impl ExecutableBuilder<Root> {
    pub fn new() -> Self {
        Self { wrapper: Root }
    }
}

impl<W> ExecutableBuilder<W> {
    pub fn wrap<T>(self, wrapper: T) -> ExecutableBuilder<stack::Stack<T, W>> {
        ExecutableBuilder {
            wrapper: stack::Stack::new(wrapper, self.wrapper),
        }
    }

    pub fn state<T>(self, state: T) -> ExecutableBuilder<stack::Stack<state::StateWrapper<T>, W>> {
        self.wrap(state::StateWrapper::new(state))
    }

    pub fn checkpoint_root<S>(
        self,
        checkpoint_manager: CheckpointManager,
        should_restore: S,
    ) -> ExecutableBuilder<stack::Stack<checkpoint::CheckpointRootWrapper<S>, W>> {
        self.wrap(checkpoint::CheckpointRootWrapper::new(
            checkpoint_manager,
            should_restore,
        ))
    }

    pub fn checkpoint(self) -> ExecutableBuilder<stack::Stack<checkpoint::CheckpointWrapper, W>> {
        self.wrap(checkpoint::CheckpointWrapper)
    }

    pub fn map<M, I>(
        self,
        mapper: M,
    ) -> ExecutableBuilder<stack::Stack<map::MapInputWrapper<M, I>, W>> {
        self.wrap(map::MapInputWrapper::new(mapper))
    }

    pub fn into<I>(
        self,
    ) -> ExecutableBuilder<stack::Stack<map::MapInputWrapper<map::IntoMapper, I>, W>> {
        self.wrap(map::MapInputWrapper::new(map::IntoMapper))
    }

    pub fn concurrency<T>(
        self,
        max: usize,
    ) -> ExecutableBuilder<stack::Stack<concurrency::ConcurrencyWrapper<T, DefaultControl>, W>>
    {
        self.wrap(concurrency::ConcurrencyWrapper::new(max))
    }

    pub fn concurrency_control<T, C>(
        self,
        max: usize,
        concurrency_control: C,
    ) -> ExecutableBuilder<stack::Stack<concurrency::ConcurrencyWrapper<T, C>, W>> {
        self.wrap(concurrency::ConcurrencyWrapper::with_concurrency_control(
            max,
            concurrency_control,
        ))
    }

    pub fn consistency<P>(
        self,
        consistency_picker: P,
        sample_size: usize,
        max_concurrency: usize,
    ) -> ExecutableBuilder<stack::Stack<consistency::ConsistencyWrapper<P>, W>> {
        self.wrap(consistency::ConsistencyWrapper::new(
            consistency_picker,
            sample_size,
            max_concurrency,
        ))
    }

    pub fn chain(
        self,
    ) -> ExecutableBuilder<stack::Stack<chain::ChainWrapper<chain::NoopMapper>, W>> {
        self.wrap(chain::ChainWrapper::new(chain::NoopMapper))
    }

    pub fn chain_map<M>(
        self,
        mapper: M,
    ) -> ExecutableBuilder<stack::Stack<chain::ChainWrapper<M>, W>> {
        self.wrap(chain::ChainWrapper::new(mapper))
    }

    pub fn react<A, RF, IF>(
        self,
        act: A,
        response_fold: RF,
        input_fold: IF,
        max_iterations: usize,
    ) -> ExecutableBuilder<stack::Stack<react::ReasonActWrapper<A, RF, IF>, W>> {
        self.wrap(react::ReasonActWrapper::new(
            act,
            response_fold,
            input_fold,
            max_iterations,
        ))
    }

    pub fn cache_with<S>(
        self,
        storage: S,
    ) -> ExecutableBuilder<stack::Stack<cache::CacheWrapper<S>, W>> {
        self.wrap(cache::CacheWrapper::new(storage))
    }

    pub fn export_with<E>(
        self,
        exporter: E,
    ) -> ExecutableBuilder<stack::Stack<export::ExportWrapper<E>, W>> {
        self.wrap(export::ExportWrapper::new(exporter))
    }

    pub fn executable<E>(&self, executable: E) -> W::Wrapper
    where
        W: Wrap<E>,
    {
        self.wrapper.wrap(executable)
    }
}

impl<E, W> Wrap<E> for ExecutableBuilder<W>
where
    W: Wrap<E>,
{
    type Wrapper = W::Wrapper;

    fn wrap(&self, inner: E) -> Self::Wrapper {
        self.wrapper.wrap(inner)
    }
}
