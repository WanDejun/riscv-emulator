use crate::config::arch_config::WordType;

pub trait Cacheable: Clone + Copy {
    /// Log2 of the cached object's minimum size in bytes (e.g., `1` for 2-byte RISC-V instructions),
    ///  not of the cached result (e.g., [`DecodeInstr`]).
    const ADDR_SHIFT_BITS: usize;

    /// Convert cacheable object's address to it's index,
    /// which is address right shifted by [`Self::ADDR_SHIFT_BITS`].
    #[inline]
    fn index_of(addr: WordType) -> usize {
        (addr as usize) >> Self::ADDR_SHIFT_BITS
    }
}

pub(super) trait Cache<T: Cacheable> {
    fn new() -> Self;
    fn get(&self, addr: WordType) -> Option<T>;
    fn put(&mut self, addr: WordType, data: T);
    fn invalidate(&mut self, addr: WordType);
    fn clear(&mut self);
}

pub(super) struct DirectCache<T, const N: usize> {
    cache: [(WordType, Option<T>); N],
}

impl<T: Cacheable, const N: usize> DirectCache<T, N> {
    #[inline]
    fn get_id(addr: WordType) -> usize {
        T::index_of(addr) & (N - 1)
    }
}

impl<T: Cacheable, const N: usize> Cache<T> for DirectCache<T, N> {
    fn new() -> Self {
        debug_assert!(N > 0 && (N & (N - 1)) == 0, "N must be a power of two");
        Self {
            cache: [(0, None); N],
        }
    }

    #[inline]
    fn get(&self, addr: WordType) -> Option<T> {
        let (tag, data) = &self.cache[Self::get_id(addr)];
        if *tag == addr { data.clone() } else { None }
    }

    #[inline]
    fn put(&mut self, addr: WordType, data: T) {
        self.cache[Self::get_id(addr)] = (addr, Some(data));
    }

    #[inline]
    fn invalidate(&mut self, addr: WordType) {
        self.cache[Self::get_id(addr)] = (0, None);
    }

    #[inline]
    fn clear(&mut self) {
        self.cache.fill((0, None));
    }
}

/// Helper struct for set-associative cache, representing one set with W ways.
struct CacheSet<T: Cacheable, const W: usize> {
    replace_idx: usize,
    source_addr: [WordType; W],
    data: [Option<T>; W],
}

impl<T: Cacheable, const W: usize> CacheSet<T, W> {
    const fn new() -> Self {
        Self {
            replace_idx: 0,
            source_addr: [0; W],
            data: [None; W],
        }
    }

    #[inline]
    fn insert(&mut self, addr: WordType, data: T) {
        self.source_addr[self.replace_idx] = addr;
        self.data[self.replace_idx] = Some(data);

        self.replace_idx += 1;
        if self.replace_idx == W {
            self.replace_idx = 0;
        }
    }

    fn invalidate(&mut self, addr: WordType) {
        if let Some(index) = self.source_addr.iter().position(|&item| item == addr) {
            self.source_addr[index] = 0;
            self.data[index] = None;
        }
    }
}

/// Set-associative cache with S sets and W ways per set.
///
/// TODO: Current replacement policy is FIFO, which is not optimal.
///
/// Example:
/// ```
/// SetCache<DecodeInstr, 4, 2> // 4 sets, 2 ways per set
/// ```
pub(super) struct SetCache<I: Cacheable, const S: usize, const W: usize> {
    cache: [CacheSet<I, W>; S],
}

impl<T: Cacheable, const S: usize, const W: usize> SetCache<T, S, W> {
    #[inline]
    fn set_index_of(addr: WordType) -> usize {
        (T::index_of(addr)) & (S - 1)
    }
}

impl<T: Cacheable, const S: usize, const W: usize> Cache<T> for SetCache<T, S, W> {
    fn new() -> Self {
        debug_assert!(S > 0 && (S & (S - 1)) == 0, "S must be a power of two.");
        debug_assert!(W > 0 && (W & (W - 1)) == 0, "W must be a power of two.");

        Self {
            cache: std::array::from_fn(|_| CacheSet::new()),
        }
    }

    #[inline]
    fn get(&self, addr: WordType) -> Option<T> {
        let set = &self.cache[Self::set_index_of(addr)];

        set.source_addr
            .iter()
            .position(|&item| item == addr)
            .and_then(|index| set.data[index].clone())
    }

    #[inline]
    fn put(&mut self, addr: WordType, data: T) {
        self.cache[Self::set_index_of(addr)].insert(addr, data);
    }

    #[inline]
    fn invalidate(&mut self, addr: WordType) {
        self.cache[Self::set_index_of(addr)].invalidate(addr);
    }

    #[inline]
    fn clear(&mut self) {
        self.cache = std::array::from_fn(|_| CacheSet::new());
    }
}

mod tests {
    use super::*;

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    struct MockCacheable(u32);

    impl Cacheable for MockCacheable {
        const ADDR_SHIFT_BITS: usize = 0;
    }

    fn test_cache_common<C: Cache<MockCacheable>>() {
        let mut cache = C::new();

        assert_eq!(cache.get(0), None);

        cache.put(0, MockCacheable(42));
        assert_eq!(cache.get(0), Some(MockCacheable(42)));

        cache.invalidate(0);
        assert_eq!(cache.get(0), None);

        cache.put(1, MockCacheable(100));
        cache.put(2, MockCacheable(200));
        assert_eq!(cache.get(1), Some(MockCacheable(100)));
        assert_eq!(cache.get(2), Some(MockCacheable(200)));

        cache.invalidate(1);
        assert_eq!(cache.get(1), None);

        cache.clear();
        assert_eq!(cache.get(1), None);
        assert_eq!(cache.get(2), None);
    }

    #[test]
    fn common_cache_tests() {
        test_cache_common::<DirectCache<MockCacheable, 8>>();
        test_cache_common::<SetCache<MockCacheable, 4, 2>>();
    }

    #[test]
    fn set_cache_test() {
        let mut cache = SetCache::<MockCacheable, 4, 2>::new();

        for i in 0..4 {
            cache.put(i, MockCacheable(i as u32));
        }

        cache.put(8, MockCacheable(8));

        for i in 0..4 {
            assert_eq!(cache.get(i), Some(MockCacheable(i as u32)));
        }

        assert_eq!(cache.get(8), Some(MockCacheable(8)));
    }
}
