#![no_std]

use core::{f64, marker::PhantomData, mem::take, ptr::NonNull};

use alloc::{boxed::Box, collections::btree_map::BTreeMap};
use spin::Mutex;
extern crate alloc;
const MASK: u64 = 0xffffffff;
static NANS: spin::Mutex<BTreeMap<u64, Entry>> = Mutex::new(BTreeMap::new());
enum Entry {
    F64 { float: f64, refc: usize },
    Ptr(NonNull<()>),
}
unsafe impl Send for Entry {}
unsafe impl Sync for Entry {}

pub union NanBox<T> {
    raw: f64,
    raw_u64: u64,
    phantom: PhantomData<T>,
}
impl<T> NanBox<T> {
    pub fn new(a: f64) -> Self {
        if a.is_nan() {
            let mut l = NANS.lock();
            let mut i = 0;
            while l.contains_key(&i) {
                i += 1
            }
            l.insert(i, Entry::F64 { float: a, refc: 1 });
            let b = a.to_bits();
            unsafe {
                Self {
                    raw_u64: (b & !MASK) | (i as u64),
                }
            }
        } else {
            unsafe { Self { raw: a } }
        }
    }
    pub fn as_f64(&self) -> Option<f64> {
        let i = unsafe {
            if !self.raw.is_nan() {
                return Some(self.raw);
            };
            let i = self.raw_u64 & MASK;
            // i as u32
            i
        };
        let mut l = NANS.lock();
        let Entry::F64 { float, .. } = l.get(&i)? else {
            return None;
        };
        Some(*float)
    }
    pub fn with_val(t: T) -> Self {
        let mut l = NANS.lock();
        let mut i = 0;
        while l.contains_key(&i) {
            i += 1
        }
        l.insert(
            i,
            Entry::Ptr(NonNull::new(Box::leak(Box::new(t)) as *mut T as *mut ()).unwrap()),
        );
        let b = f64::NAN.to_bits();
        unsafe {
            Self {
                raw_u64: (b & !MASK) | (i as u64),
            }
        }
    }
    pub fn as_ref(&self) -> Option<&T> {
        let i = unsafe {
            if !self.raw.is_nan() {
                return None;
            };
            let i = self.raw_u64 & MASK;
            // i as u32
            i
        };
        let mut l = NANS.lock();
        let Entry::Ptr(p) = l.get(&i)? else {
            return None;
        };
        Some(unsafe { p.cast().as_ref() })
    }
    pub fn as_mut(&mut self) -> Option<&mut T> {
        let i = unsafe {
            if !self.raw.is_nan() {
                return None;
            };
            let i = self.raw_u64 & MASK;
            // i as u32
            i
        };
        let mut l = NANS.lock();
        let Entry::Ptr(p) = l.get(&i)? else {
            return None;
        };
        Some(unsafe { p.cast().as_mut() })
    }
    pub fn into_inner(mut self) -> Result<T, Self> {
        let r = take(unsafe { &mut self.raw_u64 });
        if !f64::from_bits(r).is_nan() {
            unsafe {
                self.raw_u64 = r;
            };
            return Err(self);
        };
        let i = (r & MASK);
        let mut l = NANS.lock();
        let p = match l.remove(&i) {
            Some(Entry::Ptr(p)) => p,
            mut p => {
                // if let Some(Entry::F64 { float, refc })
                while let Some(p2) = p.take() {
                    p = l.insert(i, p2);
                }
                unsafe {
                    self.raw_u64 = r;
                };
                return Err(self);
            }
        };
        let a = unsafe { Box::from_raw(p.as_ptr() as *mut T) };
        Ok(*a)
    }
    pub unsafe fn from_raw(a: f64) -> Self {
        Self { raw: a }
    }
    pub unsafe fn raw(&self) -> f64 {
        unsafe { self.raw }
    }
    pub unsafe fn take_raw(&mut self) -> f64 {
        unsafe { take(&mut self.raw) }
    }
}
impl<T: Clone> Clone for NanBox<T> {
    fn clone(&self) -> Self {
        if unsafe { self.raw }.is_nan() {
            let i = {
                let i = unsafe { self.raw_u64 } & MASK;
                // i as u32
                i
            };
            let mut l = NANS.lock();
            match l.get_mut(&i) {
                None => Self {
                    raw: unsafe { self.raw },
                },
                Some(x) => match x {
                    Entry::F64 { float, refc } => {
                        *refc += 1;
                        Self {
                            raw: unsafe { self.raw },
                        }
                    }
                    Entry::Ptr(non_null) => {
                        let p = non_null.as_ptr() as *mut T;
                        drop(l);
                        Self::with_val(unsafe { &*p }.clone())
                    }
                },
            }
        } else {
            Self {
                raw: unsafe { self.raw },
            }
        }
    }
}
impl<T> Drop for NanBox<T> {
    fn drop(&mut self) {
        let _ = {
            let i = unsafe {
                if !self.raw.is_nan() {
                    return;
                };
                let i = self.raw_u64 & MASK;
                // i as u32
                i
            };
            let mut l = NANS.lock();
            let Some(e) = l.remove(&i) else {
                return;
            };
            match e {
                Entry::Ptr(p) => unsafe { Box::from_raw(p.as_ptr() as *mut T) },
                Entry::F64 { float, refc } => {
                    if refc != 0 {
                        l.insert(
                            i,
                            Entry::F64 {
                                float,
                                refc: refc - 1,
                            },
                        );
                    }
                    return;
                }
            }
        };
    }
}
#[cfg(feature = "dumpster")]
const _: () = {
    unsafe impl<T: dumpster::Trace> dumpster::Trace for NanBox<T> {
        fn accept<V: dumpster::Visitor>(&self, visitor: &mut V) -> Result<(), ()> {
            match self.as_ref() {
                None => Ok(()),
                Some(v) => v.accept(visitor),
            }
        }
    }
};
