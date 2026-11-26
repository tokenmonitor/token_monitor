//! Polling-loop behaviour around in-flight refreshes, cancellation, custom
//! backoff, an explicit runtime handle and `Retry-After` ordering.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokenmon_core::clock::Clock;
use tokenmon_core::polling::testing::ManualScheduler;
use tokenmon_core::polling::{BackoffTimer, PollOutcome, PollingLoop};

fn secs(s: u64) -> Duration {
    Duration::from_secs(s)
}

type Gated = (Arc<PollingLoop>, Arc<tokio::sync::Notify>, Arc<AtomicUsize>);

/// A loop whose refresh parks on a gate; `count` records refresh starts.
fn gated_loop(time: &ManualScheduler) -> Gated {
    let gate = Arc::new(tokio::sync::Notify::new());
    let count = Arc::new(AtomicUsize::new(0));
    let (g, c) = (Arc::clone(&gate), Arc::clone(&count));
    let l = PollingLoop::builder(
        || Some(secs(60)),
        move || {
            let (g, c) = (Arc::clone(&g), Arc::clone(&c));
            async move {
                c.fetch_add(1, Ordering::SeqCst);
                g.notified().await;
                PollOutcome::Success
            }
        },
    )
    .clock(time.clock())
    .sleeper(Arc::new(time.clone()))
    .jitter(Arc::new(|| 0.0))
    .build();
    (l, gate, count)
}

/// A manual refresh issued while another is in flight is dropped, so the
/// provider is only polled once.
#[tokio::test(flavor = "current_thread")]
async fn test_refresh_now_while_refreshing_is_a_noop() {
    let time = ManualScheduler::new();
    let (l, gate, count) = gated_loop(&time);
    let first = {
        let l = Arc::clone(&l);
        tokio::spawn(async move { l.refresh_now().await })
    };
    time.settle().await;
    assert!(l.is_refreshing());
    l.refresh_now().await;
    l.refresh_now_if_older_than(secs(0)).await;
    assert_eq!(count.load(Ordering::SeqCst), 1);
    gate.notify_one();
    first.await.expect("refresh task joins");
    assert!(!l.is_refreshing());
}

/// Dropping a refresh future mid-flight (caller cancellation) clears the
/// in-flight flag so later refreshes are not blocked forever.
#[tokio::test(flavor = "current_thread")]
async fn test_cancelled_refresh_clears_the_in_flight_flag() {
    let time = ManualScheduler::new();
    let (l, gate, count) = gated_loop(&time);
    tokio::select! {
        () = l.refresh_now() => panic!("gated refresh must not finish"),
        () = time.settle() => {}
    }
    assert!(!l.is_refreshing());
    assert!(l.last_refresh_at().is_none());
    gate.notify_one();
    l.refresh_now().await;
    assert_eq!(count.load(Ordering::SeqCst), 2);
}

/// With no refresh yet, the age-gated refresh runs; afterwards it honours the
/// age and records the completion time.
#[tokio::test(flavor = "current_thread")]
async fn test_refresh_if_older_than_runs_first_then_respects_age() {
    let time = ManualScheduler::new();
    let (l, gate, count) = gated_loop(&time);
    gate.notify_one();
    l.refresh_now_if_older_than(secs(3600)).await;
    assert_eq!(count.load(Ordering::SeqCst), 1);
    assert_eq!(l.last_refresh_at(), Some(time.clock().now()));
    l.refresh_now_if_older_than(secs(3600)).await;
    assert_eq!(count.load(Ordering::SeqCst), 1);
    time.advance(secs(3601));
    gate.notify_one();
    l.refresh_now_if_older_than(secs(3600)).await;
    assert_eq!(count.load(Ordering::SeqCst), 2);
}

/// A custom backoff replaces the 30/600 s default: waits grow 5 s, 8 s (capped).
#[tokio::test(flavor = "current_thread")]
async fn test_custom_backoff_schedule_is_used() {
    let time = ManualScheduler::new();
    let l = PollingLoop::builder(
        || Some(secs(1)),
        || async { PollOutcome::Failure { retry_after: None } },
    )
    .backoff(BackoffTimer::new(secs(5), secs(8)))
    .clock(time.clock())
    .sleeper(Arc::new(time.clone()))
    .jitter(Arc::new(|| 0.0))
    .build();
    l.start();
    assert_eq!(time.next_sleep().await, secs(5));
    time.fire();
    assert_eq!(time.next_sleep().await, secs(8));
    time.fire();
    assert_eq!(time.next_sleep().await, secs(8));
    l.stop();
}

/// A `Retry-After` shorter than the normal wait never shortens it.
#[tokio::test(flavor = "current_thread")]
async fn test_short_retry_after_does_not_shorten_the_wait() {
    let time = ManualScheduler::new();
    let l = PollingLoop::builder(
        || Some(secs(300)),
        || async {
            PollOutcome::Failure {
                retry_after: Some(secs(5)),
            }
        },
    )
    .clock(time.clock())
    .sleeper(Arc::new(time.clone()))
    .jitter(Arc::new(|| 0.0))
    .build();
    l.start();
    assert_eq!(time.next_sleep().await, secs(300));
    l.stop();
}

/// A loop built outside any runtime runs its waits on the handle it was given.
#[test]
fn test_explicit_handle_runs_waits_on_that_runtime() {
    let rt = tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("runtime builds");
    let time = ManualScheduler::new();
    let seen = Arc::new(Mutex::new(0_usize));
    let s = Arc::clone(&seen);
    let l = PollingLoop::builder(
        || Some(secs(60)),
        move || {
            *s.lock().expect("lock") += 1;
            async { PollOutcome::Success }
        },
    )
    .handle(rt.handle().clone())
    .clock(time.clock())
    .sleeper(Arc::new(time.clone()))
    .build();
    l.start();
    rt.block_on(time.settle());
    assert_eq!(*seen.lock().expect("lock"), 1);
    l.stop();
}
