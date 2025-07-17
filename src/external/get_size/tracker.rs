use std::any::Any;
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, RwLock};

/// A tracker which makes sure that shared ownership objects are only accounted for once.
pub trait GetSizeTracker {
    /// Tracks a given strong shared ownership object `strong_ref` of type `A`, which points
    /// to an arbitrary object located at `addr`.
    ///
    /// Returns `true` if the reference, as indexed by the pointed to `addr`, has not yet
    /// been seen by this tracker. Otherwise it returns false.
    ///
    /// If the `addr` has not yet been seen, the tracker __MUST__ store the `strong_ref`
    /// object to ensure that the `addr` pointed to by it remains valid for the trackers
    /// lifetime.
    fn track<A: Any + 'static, B>(&mut self, addr: *const B, strong_ref: A) -> bool;
}

impl<T: GetSizeTracker> GetSizeTracker for &mut T {
    fn track<A: Any + 'static, B>(&mut self, addr: *const B, strong_ref: A) -> bool {
        GetSizeTracker::track(*self, addr, strong_ref)
    }
}

impl<T: GetSizeTracker> GetSizeTracker for Box<T> {
    fn track<A: Any + 'static, B>(&mut self, addr: *const B, strong_ref: A) -> bool {
        GetSizeTracker::track(&mut **self, addr, strong_ref)
    }
}

impl<T: GetSizeTracker> GetSizeTracker for Mutex<T> {
    fn track<A: Any + 'static, B>(&mut self, addr: *const B, strong_ref: A) -> bool {
        let mut tracker = self.lock().unwrap();

        GetSizeTracker::track(&mut *tracker, addr, strong_ref)
    }
}

impl<T: GetSizeTracker> GetSizeTracker for RwLock<T> {
    fn track<A: Any + 'static, B>(&mut self, addr: *const B, strong_ref: A) -> bool {
        let mut tracker = self.write().unwrap();

        GetSizeTracker::track(&mut *tracker, addr, strong_ref)
    }
}

impl<T: GetSizeTracker> GetSizeTracker for Arc<Mutex<T>> {
    fn track<A: Any + 'static, B>(&mut self, addr: *const B, strong_ref: A) -> bool {
        let mut tracker = self.lock().unwrap();

        GetSizeTracker::track(&mut *tracker, addr, strong_ref)
    }
}

impl<T: GetSizeTracker> GetSizeTracker for Arc<RwLock<T>> {
    fn track<A: Any + 'static, B>(&mut self, addr: *const B, strong_ref: A) -> bool {
        let mut tracker = self.write().unwrap();

        GetSizeTracker::track(&mut *tracker, addr, strong_ref)
    }
}

/// A simple standard tracker which can be used to track shared ownership references.
#[derive(Debug, Default)]
pub struct StandardTracker {
    inner: BTreeMap<usize, Box<dyn Any + 'static>>,
}

impl StandardTracker {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn clear(&mut self) {
        self.inner.clear();
    }
}

impl GetSizeTracker for StandardTracker {
    fn track<A: Any + 'static, B>(&mut self, addr: *const B, strong_ref: A) -> bool {
        let addr = addr as usize;

        if self.inner.contains_key(&addr) {
            false
        } else {
            let strong_ref: Box<dyn Any + 'static> = Box::new(strong_ref);
            self.inner.insert(addr, strong_ref);
            true
        }
    }
}

/// A pseudo tracker which does not track anything.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoTracker {
    answer: bool,
}

impl NoTracker {
    /// Creates a new pseudo tracker, which will always return the given `answer`.
    pub fn new(answer: bool) -> Self {
        Self { answer }
    }

    /// Get the answer which will always be returned by this pseudo tracker.
    pub fn answer(&self) -> bool {
        self.answer
    }

    /// Changes the answer which will always be returned by this pseudo tracker.
    pub fn set_answer(&mut self, answer: bool) {
        self.answer = answer;
    }
}

impl GetSizeTracker for NoTracker {
    fn track<A: Any + 'static, B>(&mut self, _addr: *const B, _strong_ref: A) -> bool {
        self.answer
    }
}
