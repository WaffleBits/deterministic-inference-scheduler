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
    assert_eq!(first.schema_version, 3);
    assert_eq!(first.total_prompt_tokens, 48);
    assert_eq!(first.total_decode_tokens, 5);
    assert_eq!(first.total_reserved_kv_pages, 5);
    assert_eq!(first.max_queued_requests, 0);
    assert_eq!(first.max_active_requests, 2);
    assert_eq!(first.peak_kv_pressure_pct, 62.5);
    assert_eq!(first.declared_prefill_capacity_tokens, 144);
    assert_eq!(first.declared_decode_capacity_tokens, 6);
    assert_eq!(first.kv_page_capacity_ticks, 24);
    assert_eq!(first.queue_pressure_ratio, 0.0);
    assert_eq!(first.active_capacity_ratio, 0.333333);
    assert_eq!(first.prefill_capacity_utilization, 0.333333);
    assert_eq!(first.decode_capacity_utilization, 0.833333);
    assert!(first.kv_page_occupancy_ratio > 0.0);
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

#[test]
fn pressure_replay_reports_queue_and_capacity_summary() {
    let input: ReplayInput =
        serde_json::from_str(include_str!("../fixtures/workload_pressure.json")).unwrap();

    let report = run_replay(input).unwrap();

    assert_eq!(report.schema_version, 3);
    assert_eq!(report.request_count, report.completed_requests);
    assert_eq!(report.total_prompt_tokens, 432);
    assert_eq!(report.total_decode_tokens, 48);
    assert_eq!(report.total_reserved_kv_pages, 35);
    assert!(report.max_queued_requests >= 3);
    assert_eq!(
        report.max_active_requests,
        report.config.max_active_requests
    );
    assert!(report.peak_kv_pressure_pct > 80.0);
    assert!(report.ticks_with_queue_pressure > 0);
    assert!(report.ticks_at_active_capacity > 0);
    assert!(report.queue_pressure_ratio > 0.0);
    assert!(report.active_capacity_ratio > 0.0);
    assert!(report.prefill_capacity_utilization > 0.0);
    assert!(report.decode_capacity_utilization > 0.0);
    assert!(report.kv_page_occupancy_ratio > 0.0);
    assert_eq!(report.final_kv_pages, 0);
}
