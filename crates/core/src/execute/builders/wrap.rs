pub trait Wrap<E> {
    type Wrapper;

    fn wrap(&self, inner: E) -> Self::Wrapper;
}
