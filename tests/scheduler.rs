use rust_inference_runtime::{ReplayInput, RequestSpec, Scheduler, SchedulerConfig, run_replay};

fn config() -> SchedulerConfig {
    SchedulerConfig {
        max_active_requests: 2,
        decode_batch_size: 2,
        max_prefill_tokens_per_tick: 48,
        kv_page_tokens: 16,
        total_kv_pages: 8,
    }
}

#[test]
fn replay_is_deterministic() {
    let input = ReplayInput {
        config: config(),
        requests: vec![
            RequestSpec {
                id: "normal".into(),
                prompt_tokens: 32,
                max_output_tokens: 3,
                priority: 1,
                arrival_tick: 0,
            },
            RequestSpec {
                id: "urgent".into(),
                prompt_tokens: 16,
                max_output_tokens: 2,
                priority: 5,
                arrival_tick: 0,
            },
        ],
    };

    let first = run_replay(input.clone()).unwrap();
    let second = run_replay(input).unwrap();

    assert_eq!(first, second);
    assert_eq!(first.ticks[0].admitted, vec!["urgent", "normal"]);
    assert_eq!(first.completed_requests, 2);
    assert_eq!(first.final_kv_pages, 0);
}

#[test]
fn kv_reservations_prevent_overcommit_and_release_on_completion() {
    let mut scheduler = Scheduler::new(SchedulerConfig {
        max_active_requests: 2,
        decode_batch_size: 1,
        max_prefill_tokens_per_tick: 128,
        kv_page_tokens: 16,
        total_kv_pages: 4,
    })
    .unwrap();

    for id in ["first", "second"] {
        scheduler
            .submit(RequestSpec {
                id: id.into(),
                prompt_tokens: 32,
                max_output_tokens: 8,
                priority: 1,
                arrival_tick: 0,
            })
            .unwrap();
    }

    let first_tick = scheduler.step();
    assert_eq!(first_tick.active_requests, 1);
    assert_eq!(first_tick.queued_requests, 1);
    assert!(first_tick.used_kv_pages <= 4);

    while scheduler.completed_count() < 2 {
        let tick = scheduler.step();
        assert!(tick.used_kv_pages <= 4);
    }
    assert!(scheduler.is_idle());
}

#[test]
fn round_robin_decode_prevents_active_request_starvation() {
    let mut scheduler = Scheduler::new(SchedulerConfig {
        max_active_requests: 3,
        decode_batch_size: 1,
        max_prefill_tokens_per_tick: 128,
        kv_page_tokens: 16,
        total_kv_pages: 16,
    })
    .unwrap();

    for id in ["a", "b", "c"] {
        scheduler
            .submit(RequestSpec {
                id: id.into(),
                prompt_tokens: 8,
                max_output_tokens: 2,
                priority: 1,
                arrival_tick: 0,
            })
            .unwrap();
    }

    let decoded: Vec<String> = (0..3).flat_map(|_| scheduler.step().decoded).collect();
    assert_eq!(decoded, vec!["a", "b", "c"]);
}
