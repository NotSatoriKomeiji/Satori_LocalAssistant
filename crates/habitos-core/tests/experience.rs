use habitos_core::{
    engine::Engine,
    experience::{Advice, AppOption, Interpretation, Kind, Scope},
    model::*,
    platform::{DemoPlatform, Platform},
    store::Store,
};

#[test]
fn undo_teaches_local_exception_across_sessions_without_api_or_retraining_outputs() {
    let at = now();
    let mut p = DemoPlatform::default();
    let mut e = Engine::new(Store::memory().unwrap(), true).unwrap();
    e.observe(p.snapshot().unwrap(), at).unwrap();
    for i in 0..3 {
        let mut context = e.current.clone().unwrap();
        context.session = format!("training-{i}");
        e.store
            .sample(&Sample {
                context,
                volume: 0.35,
                at: at - 3600 * (i + 1),
                source: "manual".into(),
            })
            .unwrap();
    }
    let original = Scope::from_context(e.current.as_ref().unwrap(), &p.current.device);
    for round in 0..2 {
        e.observe(p.snapshot().unwrap(), at + 6 + round * 90)
            .unwrap();
        let id = e.proposal.as_ref().unwrap().id.clone();
        e.apply(&id, true, &mut p, at + 7 + round * 90).unwrap();
        let journal = e.store.journal().unwrap().remove(0);
        e.undo(&journal.id, &mut p, at + 8 + round * 90).unwrap();
        assert_eq!(
            e.store.count_samples().unwrap(),
            3,
            "assistant output and undo are not training samples"
        );
        e.request_decision();
        e.observe(p.snapshot().unwrap(), at + 10 + round * 90)
            .unwrap();
        assert!(e.proposal.is_none());
        p.demo_scene("editor").unwrap();
        e.observe(p.snapshot().unwrap(), at + 11 + round * 90)
            .unwrap();
        p.demo_scene("game").unwrap();
        e.observe(p.snapshot().unwrap(), at + 12 + round * 90)
            .unwrap();
    }
    assert!(e
        .store
        .holds_adjustment(Kind::Volume, &original, at + 200)
        .unwrap());
    e.observe(p.snapshot().unwrap(), at + 201).unwrap();
    assert!(e.proposal.is_none());
    assert_eq!(
        e.status(at + 202).unwrap().experience.automatic_calls_today,
        0
    );
}

#[test]
fn ambiguous_choice_must_be_confirmed_and_known_choice_works_without_ai() {
    let at = now();
    let mut p = DemoPlatform::default();
    let mut e = Engine::new(Store::memory().unwrap(), true).unwrap();
    e.observe(p.snapshot().unwrap(), at).unwrap();
    e.demo_grid(at).unwrap();
    let candidates: Vec<_> = e
        .status(at)
        .unwrap()
        .quick_apps
        .into_iter()
        .take(2)
        .map(|a| AppOption {
            id: a.app_id,
            name: a.name,
        })
        .collect();
    assert_eq!(candidates.len(), 2);
    let c = e.current.clone().unwrap();
    let scope = Scope::from_context(&c, &c.device);
    e.store
        .observe_exception(Kind::AppChoice, scope.clone(), "a", candidates.clone(), at)
        .unwrap();
    let item = e
        .store
        .observe_exception(Kind::AppChoice, scope.clone(), "b", candidates.clone(), at)
        .unwrap();
    e.claim_wake(&item.id, item.revision, at + 1).unwrap();
    e.finish_wake(
        &item.id,
        item.revision,
        Some(Advice {
            interpretation: Interpretation::ChooseApp,
            app_id: Some(candidates[0].id.clone()),
            explanation: "这个程序可能符合你的选择".into(),
        }),
    )
    .unwrap();
    assert!(
        e.app_recommendation.is_none(),
        "AI response cannot launch or silently prefer anything"
    );
    e.answer_experience(&item.id, item.revision, true, &mut p, at + 2)
        .unwrap();
    let ids = candidates.iter().map(|a| a.id.clone()).collect::<Vec<_>>();
    assert_eq!(
        e.store.preferred_app(&scope, &ids, at + 3).unwrap(),
        Some(ids[0].clone())
    );
    assert!(e.status(at + 3).unwrap().experience.question.is_none());
    assert!(
        e.answer_experience(&item.id, item.revision, true, &mut p, at + 4)
            .is_err(),
        "old confirmation cannot launch again"
    );
}
