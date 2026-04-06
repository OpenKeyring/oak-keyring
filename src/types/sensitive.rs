use std::mem::ManuallyDrop;
use zeroize::Zeroize;

#[derive(Zeroize)]
#[zeroize(drop)]
pub struct SecureString<T: Zeroize> {
    inner: T,
}

impl<T: Zeroize> SecureString<T> {
    pub fn new(value: T) -> Self {
        Self { inner: value }
    }

    pub fn get(&self) -> &T {
        &self.inner
    }

    pub fn into_inner(self) -> T {
        let this = ManuallyDrop::new(self);
        unsafe { std::ptr::read(&this.inner) }
    }
}

impl<T: Zeroize> Clone for SecureString<T> {
    fn clone(&self) -> Self {
        panic!("SecureString cannot be cloned");
    }
}

impl<T: Zeroize + std::fmt::Debug> std::fmt::Debug for SecureString<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "***REDACTED***")
    }
}

impl<T: Zeroize + std::fmt::Display> std::fmt::Display for SecureString<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "***REDACTED***")
    }
}

impl<T: Zeroize + serde::Serialize> serde::Serialize for SecureString<T> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.inner.serialize(serializer)
    }
}

impl<'de, T: Zeroize + serde::Deserialize<'de>> serde::Deserialize<'de> for SecureString<T> {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        T::deserialize(deserializer).map(Self::new)
    }
}

pub type SecureBytes = SecureString<Vec<u8>>;
pub type SecureStr = SecureString<String>;
