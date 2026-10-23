use proptest::prelude::*;
use proptest::test_runner::FileFailurePersistence;
use rust_inference_runtime::{ReplayInput, RequestSpec, Scheduler, SchedulerConfig, run_replay};
use std::collections::{BTreeMap, BTreeSet};

fn config() -> SchedulerConfig {
    SchedulerConfig {
        max_active_requests: 2,
        decode_batch_size: 2,
        max_prefill_tokens_per_tick: 48,
        kv_page_tokens: 16,
        total_kv_pages: 8,
    }
}

fn generated_replay_input() -> impl Strategy<Value = ReplayInput> {
    (
        1u32..65,
        1usize..9,
        1usize..9,
        1u32..129,
        prop::collection::vec((1u32..513, 1u32..129, 0u8..8, 0u64..16), 0..25),
    )
        .prop_map(
            |(kv_page_tokens, max_active_requests, decode_batch_size, prefill_limit, raw)| {
                let mut total_kv_pages = 1u32;
                let requests: Vec<RequestSpec> = raw
                    .into_iter()
                    .enumerate()
                    .map(
                        |(index, (prompt_tokens, max_output_tokens, priority, arrival_tick))| {
                            let reserved_pages = u32::try_from(
                                (u64::from(prompt_tokens) + u64::from(max_output_tokens))
                                    .div_ceil(u64::from(kv_page_tokens)),
                            )
                            .expect("bounded generated tokens fit in the page counter");
                            total_kv_pages = total_kv_pages.max(reserved_pages);
                            RequestSpec {
                                id: format!("request-{index}"),
                                prompt_tokens,
                                max_output_tokens,
                                priority,
                                arrival_tick,
                            }
                        },
                    )
                    .collect();

                ReplayInput {
                    config: SchedulerConfig {
                        max_active_requests,
                        decode_batch_size,
                        max_prefill_tokens_per_tick: prefill_limit,
                        kv_page_tokens,
                        total_kv_pages,
                    },
                    requests,
                }
            },
        )
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 128,
        failure_persistence: Some(Box::new(FileFailurePersistence::WithSource("proptest-regressions"))),
        .. ProptestConfig::default()
    })]

    #[test]
    fn generated_replays_preserve_determinism_and_accounting(input in generated_replay_input()) {
        let requests = input.requests.clone();
        let config = input.config.clone();
        let report = run_replay(input.clone()).expect("generated inputs are valid");
        let repeated = run_replay(input).expect("replay should be repeatable");
        prop_assert_eq!(&report, &repeated);

        let prompt_by_id: BTreeMap<String, u32> = requests
            .iter()
            .map(|request| (request.id.clone(), request.prompt_tokens))
            .collect();
        let expected_outputs: BTreeMap<String, u32> = requests
            .iter()
            .map(|request| (request.id.clone(), request.max_output_tokens))
            .collect();
        let expected_ids: BTreeSet<String> = requests
            .iter()
            .map(|request| request.id.clone())
            .collect();

        let mut admitted = Vec::new();
        let mut completed = Vec::new();
        let mut decoded_outputs = BTreeMap::<String, u32>::new();
        for (index, tick) in report.ticks.iter().enumerate() {
            prop_assert_eq!(tick.tick, index as u64);
            prop_assert!(tick.active_requests <= config.max_active_requests);
            prop_assert!(tick.used_kv_pages <= config.total_kv_pages);
            prop_assert_eq!(tick.decoded_tokens as usize, tick.decoded.len());

            let prefill_tokens: u32 = tick
                .admitted
                .iter()
                .map(|id| *prompt_by_id.get(id).expect("admitted ID belongs to input"))
                .sum();
            prop_assert_eq!(tick.admitted_prefill_tokens, prefill_tokens);
            prop_assert!(tick.completed.iter().all(|id| tick.decoded.iter().any(|decoded| decoded == id)));

            admitted.extend(tick.admitted.iter().cloned());
            completed.extend(tick.completed.iter().cloned());
            for id in &tick.decoded {
                *decoded_outputs.entry(id.clone()).or_default() += 1;
            }

            let arrived = requests
                .iter()
                .filter(|request| request.arrival_tick <= tick.tick)
                .count();
            let completed_so_far: usize = report.ticks[..=index]
                .iter()
                .map(|prior| prior.completed.len())
                .sum();
            prop_assert_eq!(tick.queued_requests + tick.active_requests + completed_so_far, arrived);
        }

        admitted.sort();
        completed.sort();
        let expected_ids: Vec<String> = expected_ids.into_iter().collect();
        prop_assert_eq!(&admitted, &expected_ids);
        prop_assert_eq!(&completed, &expected_ids);
        prop_assert_eq!(decoded_outputs, expected_outputs);

        let expected_prompt_tokens: u64 = requests.iter().map(|request| u64::from(request.prompt_tokens)).sum();
        let expected_decode_tokens: u64 = requests.iter().map(|request| u64::from(request.max_output_tokens)).sum();
        let expected_reserved_pages: u64 = requests
            .iter()
            .map(|request| {
                (u64::from(request.prompt_tokens) + u64::from(request.max_output_tokens))
                    .div_ceil(u64::from(config.kv_page_tokens))
            })
            .sum();

        prop_assert_eq!(report.request_count, requests.len());
        prop_assert_eq!(report.completed_requests, requests.len());
        prop_assert_eq!(report.total_prompt_tokens, expected_prompt_tokens);
        prop_assert_eq!(report.total_decode_tokens, expected_decode_tokens);
        prop_assert_eq!(report.total_reserved_kv_pages, expected_reserved_pages);
        prop_assert_eq!(report.total_ticks, report.ticks.len() as u64);
        prop_assert_eq!(report.final_kv_pages, 0);
        prop_assert_eq!(
            report.peak_kv_pages,
            report.ticks.iter().map(|tick| tick.used_kv_pages).max().unwrap_or(0)
        );
        prop_assert_eq!(
            report.max_queued_requests,
            report.ticks.iter().map(|tick| tick.queued_requests).max().unwrap_or(0)
        );
        prop_assert_eq!(
            report.max_active_requests,
            report.ticks.iter().map(|tick| tick.active_requests).max().unwrap_or(0)
        );
        prop_assert!(report.max_active_requests <= config.max_active_requests);
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
fn rejects_kv_reservation_whose_token_sum_exceeds_u32() {
    let mut scheduler = Scheduler::new(SchedulerConfig {
        max_active_requests: 1,
        decode_batch_size: 1,
        max_prefill_tokens_per_tick: u32::MAX,
        kv_page_tokens: 1,
        total_kv_pages: u32::MAX,
    })
    .unwrap();

    let error = scheduler
        .submit(RequestSpec {
            id: "overflowing-reservation".into(),
            prompt_tokens: u32::MAX,
            max_output_tokens: 1,
            priority: 0,
            arrival_tick: 0,
        })
        .unwrap_err();

    assert!(error.to_string().contains("requires more KV pages"));
}

#[test]
fn kv_capacity_check_does_not_admit_a_request_past_u32_capacity() {
    let mut scheduler = Scheduler::new(SchedulerConfig {
        max_active_requests: 2,
        decode_batch_size: 1,
        max_prefill_tokens_per_tick: u32::MAX,
        kv_page_tokens: 1,
        total_kv_pages: u32::MAX,
    })
    .unwrap();
    for id in ["first", "second"] {
        scheduler
            .submit(RequestSpec {
                id: id.into(),
                prompt_tokens: 2_147_483_647,
                max_output_tokens: 2,
                priority: 0,
                arrival_tick: 0,
            })
            .unwrap();
    }

    let first_tick = scheduler.step();
    assert_eq!(first_tick.admitted, vec!["first"]);
    assert_eq!(first_tick.used_kv_pages, 2_147_483_649);

    let second_tick = scheduler.step();
    assert!(second_tick.admitted.is_empty());
    assert_eq!(second_tick.queued_requests, 1);
    assert_eq!(second_tick.used_kv_pages, 0);

    let third_tick = scheduler.step();
    assert_eq!(third_tick.admitted, vec!["second"]);
    assert_eq!(third_tick.used_kv_pages, 2_147_483_649);
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
