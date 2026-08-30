//! 样本存储：按 series 组织的环形缓冲。
//!
//! 写端：适配器任务（[`SampleStore::push`]）；读端：UI 主线程
//! （[`SampleStore::latest`] / [`SampleStore::history`]）。
//! M1 用 `RwLock` 起步，预留替换点（arc-swap / 分片锁），性能不足再升级。

use std::collections::HashMap;
use std::sync::{RwLock, RwLockReadGuard, RwLockWriteGuard};

use crate::adapter::{Sample, SeriesId};

/// 单条 series 的环形缓冲，满则覆盖最旧。
#[derive(Debug)]
pub struct RingBuffer {
    /// 容量上限。
    capacity: usize,
    /// 元素按写入顺序存放，长度 <= 容量。
    buf: Vec<Sample>,
    /// 逻辑起始位置（满后随覆盖前移）。
    start: usize,
}

impl RingBuffer {
    /// 构造指定容量的环形缓冲；容量必须大于 0。
    pub fn new(capacity: usize) -> Self {
        assert!(capacity > 0, "环形缓冲容量必须大于 0");
        Self {
            capacity,
            buf: Vec::with_capacity(capacity),
            start: 0,
        }
    }

    /// 写入一个样本；已满时覆盖最旧。
    pub fn push(&mut self, sample: Sample) {
        if self.buf.len() < self.capacity {
            self.buf.push(sample);
        } else {
            self.buf[self.start] = sample;
            self.start = (self.start + 1) % self.capacity;
        }
    }

    /// 当前存储的样本数。
    pub fn len(&self) -> usize {
        self.buf.len()
    }

    /// 是否为空。
    pub fn is_empty(&self) -> bool {
        self.buf.is_empty()
    }

    /// 最新一个样本。
    pub fn latest(&self) -> Option<&Sample> {
        if self.buf.is_empty() {
            return None;
        }
        Some(&self.buf[(self.start + self.buf.len() - 1) % self.capacity])
    }

    /// 最近至多 `window` 个样本，按时间从旧到新。
    pub fn history(&self, window: usize) -> Vec<&Sample> {
        let take = self.buf.len().min(window);
        let mut out = Vec::with_capacity(take);
        for i in (self.buf.len() - take)..self.buf.len() {
            out.push(&self.buf[(self.start + i) % self.capacity]);
        }
        out
    }
}

/// 按 series 组织的环形缓冲集合。
#[derive(Debug)]
pub struct SampleStore {
    /// series id → 环形缓冲。
    series: RwLock<HashMap<SeriesId, RingBuffer>>,
    /// 新建 series 的默认容量。
    default_capacity: usize,
}

impl SampleStore {
    /// 默认容量（架构 §4：默认 4096 样本）。
    pub const DEFAULT_CAPACITY: usize = 4096;

    /// 以默认容量构造。
    pub fn new() -> Self {
        Self::with_capacity(Self::DEFAULT_CAPACITY)
    }

    /// 以指定容量构造（作用于此后新建的 series）。
    /// 以指定容量构造（作用于此后新建的 series）；容量至少为 1。
    pub fn with_capacity(default_capacity: usize) -> Self {
        Self {
            series: RwLock::new(HashMap::new()),
            default_capacity: default_capacity.max(1),
        }
    }

    /// 写入一个样本（适配器侧调用；锁被毒化时沿用旧数据，不 panic）。
    pub fn push(&self, series: SeriesId, sample: Sample) {
        let mut map = self.write();
        let buf = map
            .entry(series)
            .or_insert_with(|| RingBuffer::new(self.default_capacity));
        buf.push(sample);
    }

    /// 读取指定 series 的最新样本。
    pub fn latest(&self, series: &SeriesId) -> Option<Sample> {
        self.read()
            .get(series)
            .and_then(RingBuffer::latest)
            .cloned()
    }

    /// 读取指定 series 最近至多 `window` 个样本（按时间从旧到新）。
    pub fn history(&self, series: &SeriesId, window: usize) -> Vec<Sample> {
        self.read()
            .get(series)
            .map(|buf| buf.history(window).into_iter().cloned().collect())
            .unwrap_or_default()
    }

    /// 当前持有的全部 series id。
    pub fn series_ids(&self) -> Vec<SeriesId> {
        self.read().keys().cloned().collect()
    }

    fn read(&self) -> RwLockReadGuard<'_, HashMap<SeriesId, RingBuffer>> {
        self.series.read().unwrap_or_else(|e| e.into_inner())
    }

    fn write(&self) -> RwLockWriteGuard<'_, HashMap<SeriesId, RingBuffer>> {
        self.series.write().unwrap_or_else(|e| e.into_inner())
    }
}

impl Default for SampleStore {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::SystemTime;

    fn sample(seq: u64) -> Sample {
        Sample {
            timestamp: SystemTime::now(),
            value: crate::adapter::SampleValue::Number(seq as f64),
        }
    }

    fn series(metric: &str) -> SeriesId {
        SeriesId::new(&crate::adapter::AdapterId::new("test.adapter"), metric)
    }

    #[test]
    fn ring_buffer_keeps_capacity_and_overwrites_oldest() {
        let mut buf = RingBuffer::new(3);
        for seq in 0..5 {
            buf.push(sample(seq));
        }
        assert_eq!(buf.len(), 3);
        // 覆盖后应保留 2、3、4
        let values: Vec<u64> = buf
            .history(3)
            .iter()
            .map(|s| match s.value {
                crate::adapter::SampleValue::Number(v) => v as u64,
                _ => unreachable!(),
            })
            .collect();
        assert_eq!(values, vec![2, 3, 4]);
    }

    #[test]
    fn ring_buffer_latest_is_last_pushed() {
        let mut buf = RingBuffer::new(2);
        buf.push(sample(1));
        buf.push(sample(2));
        buf.push(sample(3));
        let latest = buf.latest().expect("有样本");
        match latest.value {
            crate::adapter::SampleValue::Number(v) => assert_eq!(v, 3.0),
            _ => unreachable!(),
        }
    }

    #[test]
    fn ring_buffer_history_respects_window() {
        let mut buf = RingBuffer::new(16);
        for seq in 0..10 {
            buf.push(sample(seq));
        }
        assert_eq!(buf.history(100).len(), 10);
        assert_eq!(buf.history(3).len(), 3);
    }

    #[test]
    fn store_tracks_multiple_series() {
        let store = SampleStore::with_capacity(8);
        let s1 = series("a");
        let s2 = series("b");
        store.push(s1.clone(), sample(1));
        store.push(s1.clone(), sample(2));
        store.push(s2.clone(), sample(9));

        assert_eq!(store.series_ids().len(), 2);
        assert!(store.latest(&s1).is_some());
        assert_eq!(store.history(&s1, 10).len(), 2);
        assert_eq!(store.history(&s2, 10).len(), 1);
        assert_eq!(store.history(&series("c"), 10).len(), 0);
    }
}
