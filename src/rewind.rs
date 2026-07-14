struct RewindEntry<T> {
    time: u64,
    data: T,
}

impl<T> RewindEntry<T> {
    fn new(time: u64, data: T) -> Self {
        Self { time, data }
    }
}

/// The entries are never dropped!
pub struct RewindBuffer<T> {
    buf: Box<[Option<RewindEntry<T>>]>,
    pos: usize,
}

impl<T> RewindBuffer<T> {
    pub fn new(capacity: usize) -> Self {
        let vec = Vec::<RewindEntry<T>>::with_capacity(capacity);
        let (buf, _, cap) = vec.into_raw_parts();
        unsafe {
            buf.write_bytes(0, cap);
        }
        Self { buf, cap, pos: 0 }
    }

    unsafe fn time_of(&self, i: usize) -> u64 {
        unsafe {
            self.buf
                .add(i)
                .add(std::mem::offset_of!(RewindEntry<T>, time))
                .cast::<u64>()
                .read()
        }
    }

    unsafe fn data_of(&self, i: usize) -> *mut T {
        unsafe {
            self.buf
                .add(i)
                .add(std::mem::offset_of!(RewindEntry<T>, data))
                .cast::<T>()
        }
    }

    fn next(&self, i: usize) -> usize {
        match i + 1 {
            n if n == self.cap => 0,
            n => n,
        }
    }

    fn prev(&self, i: usize) -> usize {
        match i {
            0 => self.cap - 1,
            n @ 1.. => n - 1,
        }
    }

    pub fn insert(&mut self, time: u64, entry: T) {
        assert!(time >= self.newest_time());
        unsafe {
            self.buf.add(self.pos).write(RewindEntry::new(time, entry));
        }
        self.pos += 1;
        if self.pos > self.cap {
            self.pos = 0;
        }
    }

    pub fn newest_time(&self) -> u64 {
        unsafe { self.time_of(self.prev(self.pos)) }
    }

    pub fn oldest_time(&self) -> u64 {
        unsafe { self.time_of(self.pos) }
    }

    pub fn rewind(&mut self, time: u64, mut cb: impl FnMut(&T)) {
        assert!(time >= self.oldest_time());

        let mut i = self.pos;
        loop {
            i = self.prev(i);

            let entry_time = unsafe { self.time_of(i) };
            if entry_time <= time {
                break;
            }

            cb(unsafe { self.data_of(i).as_ref_unchecked() });
        }
    }
}
