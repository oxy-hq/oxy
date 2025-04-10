use super::wrap::Wrap;

pub struct Stack<Inner, Outer> {
    inner: Inner,
    outer: Outer,
}

impl<Inner, Outer> Stack<Inner, Outer> {
    pub fn new(inner: Inner, outer: Outer) -> Self {
        Self { inner, outer }
    }
}

impl<E, Inner, Outer> Wrap<E> for Stack<Inner, Outer>
where
    Inner: Wrap<E>,
    Outer: Wrap<Inner::Wrapper>,
{
    type Wrapper = Outer::Wrapper;

    fn wrap(&self, executable: E) -> Self::Wrapper {
        let inner = self.inner.wrap(executable);

        self.outer.wrap(inner)
    }
}
