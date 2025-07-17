#![doc = include_str!("./lib.md")]
#![cfg_attr(docsrs, feature(doc_cfg))]

use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet, BinaryHeap, HashMap, HashSet, LinkedList, VecDeque};
use std::convert::Infallible;
use std::marker::{PhantomData, PhantomPinned};
use std::num::{
    NonZeroI8, NonZeroI16, NonZeroI32, NonZeroI64, NonZeroI128, NonZeroIsize, NonZeroU8,
    NonZeroU16, NonZeroU32, NonZeroU64, NonZeroU128, NonZeroUsize,
};
use std::rc::{Rc, Weak as RcWeak};
use std::sync::atomic::{
    AtomicBool, AtomicI8, AtomicI16, AtomicI32, AtomicI64, AtomicIsize, AtomicU8, AtomicU16,
    AtomicU32, AtomicU64, AtomicUsize, Ordering,
};
use std::sync::{Arc, Mutex, RwLock, Weak as ArcWeak};
use std::time::{Duration, Instant, SystemTime};

mod tracker;
pub use tracker::*;

/// Determine the size in bytes an object occupies inside RAM.
pub trait GetSize: Sized {
    /// Determines how may bytes this object occupies inside the stack.
    ///
    /// The default implementation uses [`std::mem::size_of`] and should work for almost all types.
    fn get_stack_size() -> usize {
        std::mem::size_of::<Self>()
    }

    /// Determines how many bytes this object occupies inside the heap.
    ///
    /// The default implementation returns 0, assuming the object is fully allocated on the stack.
    /// It must be adjusted as appropriate for objects which hold data inside the heap.
    fn get_heap_size(&self) -> usize {
        0
    }

    /// Determines how many bytes this object occupies inside the heap while using a `tracker`.
    ///
    /// The default implementation ignores the tracker and calls [`get_heap_size`](Self::get_heap_size)
    /// instead, returning the tracker untouched in the second argument.
    fn get_heap_size_with_tracker<T: GetSizeTracker>(&self, tracker: T) -> (usize, T) {
        (GetSize::get_heap_size(self), tracker)
    }

    /// Determines the total size of the object.
    ///
    /// The default implementation simply adds up the results of [`get_stack_size`](Self::get_stack_size)
    /// and [`get_heap_size`](Self::get_heap_size) and is not meant to be changed.
    fn get_size(&self) -> usize {
        Self::get_stack_size() + GetSize::get_heap_size(self)
    }

    /// Determines the total size of the object while using a `tracker`.
    ///
    /// The default implementation simply adds up the results of [`get_stack_size`](Self::get_stack_size)
    /// and [`get_heap_size_with_tracker`](Self::get_heap_size_with_tracker) and is not meant to
    /// be changed.
    fn get_size_with_tracker<T: GetSizeTracker>(&self, tracker: T) -> (usize, T) {
        let stack_size = Self::get_stack_size();
        let (heap_size, tracker) = GetSize::get_heap_size_with_tracker(self, tracker);

        let total = stack_size + heap_size;

        (total, tracker)
    }
}

impl GetSize for () {}
impl GetSize for bool {}
impl GetSize for u8 {}
impl GetSize for u16 {}
impl GetSize for u32 {}
impl GetSize for u64 {}
impl GetSize for u128 {}
impl GetSize for usize {}
impl GetSize for NonZeroU8 {}
impl GetSize for NonZeroU16 {}
impl GetSize for NonZeroU32 {}
impl GetSize for NonZeroU64 {}
impl GetSize for NonZeroU128 {}
impl GetSize for NonZeroUsize {}
impl GetSize for i8 {}
impl GetSize for i16 {}
impl GetSize for i32 {}
impl GetSize for i64 {}
impl GetSize for i128 {}
impl GetSize for isize {}
impl GetSize for NonZeroI8 {}
impl GetSize for NonZeroI16 {}
impl GetSize for NonZeroI32 {}
impl GetSize for NonZeroI64 {}
impl GetSize for NonZeroI128 {}
impl GetSize for NonZeroIsize {}
impl GetSize for f32 {}
impl GetSize for f64 {}
impl GetSize for char {}

impl GetSize for AtomicBool {}
impl GetSize for AtomicI8 {}
impl GetSize for AtomicI16 {}
impl GetSize for AtomicI32 {}
impl GetSize for AtomicI64 {}
impl GetSize for AtomicIsize {}
impl GetSize for AtomicU8 {}
impl GetSize for AtomicU16 {}
impl GetSize for AtomicU32 {}
impl GetSize for AtomicU64 {}
impl GetSize for AtomicUsize {}
impl GetSize for Ordering {}

impl GetSize for std::cmp::Ordering {}

impl GetSize for Infallible {}
impl<T> GetSize for PhantomData<T> {}
impl GetSize for PhantomPinned {}

impl GetSize for Instant {}
impl GetSize for Duration {}
impl GetSize for SystemTime {}

impl<'a, T> GetSize for Cow<'a, T>
where
    T: ToOwned,
    <T as ToOwned>::Owned: GetSize,
{
    fn get_heap_size(&self) -> usize {
        match self {
            Self::Borrowed(_borrowed) => 0,
            Self::Owned(owned) => GetSize::get_heap_size(owned),
        }
    }
}

macro_rules! impl_size_set {
    ($name:ident) => {
        impl<T> GetSize for $name<T>
        where
            T: GetSize,
        {
            fn get_heap_size(&self) -> usize {
                let mut total = 0;

                for v in self.iter() {
                    // We assume that value are hold inside the heap.
                    total += GetSize::get_size(v);
                }

                let additional: usize = self.capacity() - self.len();
                total += additional * T::get_stack_size();

                total
            }
        }
    };
}

macro_rules! impl_size_set_no_capacity {
    ($name:ident) => {
        impl<T> GetSize for $name<T>
        where
            T: GetSize,
        {
            fn get_heap_size(&self) -> usize {
                let mut total = 0;

                for v in self.iter() {
                    // We assume that value are hold inside the heap.
                    total += GetSize::get_size(v);
                }

                total
            }
        }
    };
}

macro_rules! impl_size_map {
    ($name:ident) => {
        impl<K, V> GetSize for $name<K, V>
        where
            K: GetSize,
            V: GetSize,
        {
            fn get_heap_size(&self) -> usize {
                let mut total = 0;

                for (k, v) in self.iter() {
                    // We assume that keys and value are hold inside the heap.
                    total += GetSize::get_size(k);
                    total += GetSize::get_size(v);
                }

                let additional: usize = self.capacity() - self.len();
                total += additional * K::get_stack_size();
                total += additional * V::get_stack_size();

                total
            }
        }
    };
}

macro_rules! impl_size_map_no_capacity {
    ($name:ident) => {
        impl<K, V> GetSize for $name<K, V>
        where
            K: GetSize,
            V: GetSize,
        {
            fn get_heap_size(&self) -> usize {
                let mut total = 0;

                for (k, v) in self.iter() {
                    // We assume that keys and value are hold inside the heap.
                    total += GetSize::get_size(k);
                    total += GetSize::get_size(v);
                }

                total
            }
        }
    };
}

impl_size_map_no_capacity!(BTreeMap);
impl_size_set_no_capacity!(BTreeSet);
impl_size_set!(BinaryHeap);
impl_size_map!(HashMap);
impl_size_set!(HashSet);
impl_size_set_no_capacity!(LinkedList);
impl_size_set!(VecDeque);

impl_size_set!(Vec);

macro_rules! impl_size_tuple {
    ($($t:ident, $T:ident),+) => {
        impl<$($T,)*> GetSize for ($($T,)*)
        where
            $(
                $T: GetSize,
            )*
        {
            fn get_heap_size(&self) -> usize {
                let mut total = 0;

                let ($($t,)*) = self;
                $(
                    total += GetSize::get_heap_size($t);
                )*

                total
            }
        }
    }
}

macro_rules! execute_tuple_macro_16 {
    ($name:ident) => {
        $name!(v1, V1);
        $name!(v1, V1, v2, V2);
        $name!(v1, V1, v2, V2, v3, V3);
        $name!(v1, V1, v2, V2, v3, V3, v4, V4);
        $name!(v1, V1, v2, V2, v3, V3, v4, V4, v5, V5);
        $name!(v1, V1, v2, V2, v3, V3, v4, V4, v5, V5, v6, V6);
        $name!(v1, V1, v2, V2, v3, V3, v4, V4, v5, V5, v6, V6, v7, V7);
        $name!(
            v1, V1, v2, V2, v3, V3, v4, V4, v5, V5, v6, V6, v7, V7, v8, V8
        );
        $name!(
            v1, V1, v2, V2, v3, V3, v4, V4, v5, V5, v6, V6, v7, V7, v8, V8, v9, V9
        );
        $name!(
            v1, V1, v2, V2, v3, V3, v4, V4, v5, V5, v6, V6, v7, V7, v8, V8, v9, V9, v10, V10
        );
        $name!(
            v1, V1, v2, V2, v3, V3, v4, V4, v5, V5, v6, V6, v7, V7, v8, V8, v9, V9, v10, V10, v11,
            V11
        );
        $name!(
            v1, V1, v2, V2, v3, V3, v4, V4, v5, V5, v6, V6, v7, V7, v8, V8, v9, V9, v10, V10, v11,
            V11, v12, V12
        );
        $name!(
            v1, V1, v2, V2, v3, V3, v4, V4, v5, V5, v6, V6, v7, V7, v8, V8, v9, V9, v10, V10, v11,
            V11, v12, V12, v13, V13
        );
        $name!(
            v1, V1, v2, V2, v3, V3, v4, V4, v5, V5, v6, V6, v7, V7, v8, V8, v9, V9, v10, V10, v11,
            V11, v12, V12, v13, V13, v14, V14
        );
        $name!(
            v1, V1, v2, V2, v3, V3, v4, V4, v5, V5, v6, V6, v7, V7, v8, V8, v9, V9, v10, V10, v11,
            V11, v12, V12, v13, V13, v14, V14, v15, V15
        );
        $name!(
            v1, V1, v2, V2, v3, V3, v4, V4, v5, V5, v6, V6, v7, V7, v8, V8, v9, V9, v10, V10, v11,
            V11, v12, V12, v13, V13, v14, V14, v15, V15, v16, V16
        );
    };
}

execute_tuple_macro_16!(impl_size_tuple);

impl<T, const SIZE: usize> GetSize for [T; SIZE]
where
    T: GetSize,
{
    fn get_heap_size(&self) -> usize {
        let mut total = 0;

        for element in self.iter() {
            // The array stack size already accounts for the stack size of the elements of the array.
            total += GetSize::get_heap_size(element);
        }

        total
    }
}

impl<T> GetSize for &[T] where T: GetSize {}

impl<T> GetSize for &T {}
impl<T> GetSize for &mut T {}
impl<T> GetSize for *const T {}
impl<T> GetSize for *mut T {}

impl<T> GetSize for Box<T>
where
    T: GetSize,
{
    fn get_heap_size(&self) -> usize {
        GetSize::get_size(&**self)
    }
}

impl<T> GetSize for Rc<T>
where
    T: GetSize + 'static,
{
    fn get_heap_size(&self) -> usize {
        let tracker = StandardTracker::default();

        let (total, _) = GetSize::get_heap_size_with_tracker(self, tracker);

        total
    }

    fn get_heap_size_with_tracker<TR: GetSizeTracker>(&self, mut tracker: TR) -> (usize, TR) {
        let strong_ref = Rc::clone(self);

        let addr = Rc::as_ptr(&strong_ref);

        if tracker.track(addr, strong_ref) {
            GetSize::get_size_with_tracker(&**self, tracker)
        } else {
            (0, tracker)
        }
    }
}

impl<T> GetSize for RcWeak<T> {}

impl<T> GetSize for Arc<T>
where
    T: GetSize + 'static,
{
    fn get_heap_size(&self) -> usize {
        let tracker = StandardTracker::default();

        let (total, _) = GetSize::get_heap_size_with_tracker(self, tracker);

        total
    }

    fn get_heap_size_with_tracker<TR: GetSizeTracker>(&self, mut tracker: TR) -> (usize, TR) {
        let strong_ref = Arc::clone(self);

        let addr = Arc::as_ptr(&strong_ref);

        if tracker.track(addr, strong_ref) {
            GetSize::get_size_with_tracker(&**self, tracker)
        } else {
            (0, tracker)
        }
    }
}

impl<T> GetSize for ArcWeak<T> {}

impl<T> GetSize for Option<T>
where
    T: GetSize,
{
    fn get_heap_size(&self) -> usize {
        match self {
            // The options stack size already accounts for the values stack size.
            Some(t) => GetSize::get_heap_size(t),
            None => 0,
        }
    }
}

impl<T, E> GetSize for Result<T, E>
where
    T: GetSize,
    E: GetSize,
{
    fn get_heap_size(&self) -> usize {
        match self {
            // The results stack size already accounts for the values stack size.
            Ok(t) => GetSize::get_heap_size(t),
            Err(e) => GetSize::get_heap_size(e),
        }
    }
}

impl<T> GetSize for Mutex<T>
where
    T: GetSize,
{
    fn get_heap_size(&self) -> usize {
        // We assume that a Mutex does hold its data at the stack.
        GetSize::get_heap_size(&*(self.lock().unwrap()))
    }
}

impl<T> GetSize for RwLock<T>
where
    T: GetSize,
{
    fn get_heap_size(&self) -> usize {
        // We assume that a RwLock does hold its data at the stack.
        GetSize::get_heap_size(&*(self.read().unwrap()))
    }
}

impl GetSize for String {
    fn get_heap_size(&self) -> usize {
        self.capacity()
    }
}

impl GetSize for &str {}

impl GetSize for std::ffi::CString {
    fn get_heap_size(&self) -> usize {
        self.as_bytes_with_nul().len()
    }
}

impl GetSize for &std::ffi::CStr {
    fn get_heap_size(&self) -> usize {
        self.to_bytes_with_nul().len()
    }
}

impl GetSize for std::ffi::OsString {
    fn get_heap_size(&self) -> usize {
        self.len()
    }
}

impl GetSize for &std::ffi::OsStr {
    fn get_heap_size(&self) -> usize {
        self.len()
    }
}

impl GetSize for std::fs::DirBuilder {}
impl GetSize for std::fs::DirEntry {}
impl GetSize for std::fs::File {}
impl GetSize for std::fs::FileType {}
impl GetSize for std::fs::Metadata {}
impl GetSize for std::fs::OpenOptions {}
impl GetSize for std::fs::Permissions {}
impl GetSize for std::fs::ReadDir {}

impl<T> GetSize for std::io::BufReader<T>
where
    T: GetSize,
{
    fn get_heap_size(&self) -> usize {
        let mut total = GetSize::get_heap_size(self.get_ref());

        total += self.capacity();

        total
    }
}

impl<T> GetSize for std::io::BufWriter<T>
where
    T: GetSize + std::io::Write,
{
    fn get_heap_size(&self) -> usize {
        let mut total = GetSize::get_heap_size(self.get_ref());

        total += self.capacity();

        total
    }
}

impl GetSize for std::path::PathBuf {
    fn get_heap_size(&self) -> usize {
        self.capacity()
    }
}

impl GetSize for &std::path::Path {}

impl<T> GetSize for Box<[T]> {
    fn get_heap_size(&self) -> usize {
        let mut total = 0;
        for item in self.iter() {
            total += item.get_size()
        }

        total
    }
}
