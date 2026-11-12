"""Offline unit tests for the shape guards (run: python3 -m unittest
discover -s scripts -p 'test_shape_guard.py'). They cover the source
reader and the ratchet in scripts/shape_guard.py and one hit and one miss
for each rule of the four checks. The checks themselves run on the tree."""
import contextlib
import importlib.util
import io
import os
import sys
import tempfile
import unittest

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import shape_guard as sg  # noqa: E402


def load(name):
    spec = importlib.util.spec_from_file_location(name.replace("-", "_"), f"{HERE}/{name}.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


borrows = load("check-refcell-borrows")
timers = load("check-owned-timers")
requests = load("check-request-match")
enums = load("check-stage-enums")


@contextlib.contextmanager
def tree(files):
    with tempfile.TemporaryDirectory() as root:
        for rel, text in files.items():
            path = os.path.join(root, rel)
            os.makedirs(os.path.dirname(path), exist_ok=True)
            with open(path, "w", encoding="utf-8") as handle:
                handle.write(text)
        yield root


def keys(check, files):
    with tree(files) as root:
        return sorted(key for key, _, _ in check.hits(root))


def run(check, root, *flags):
    out = io.StringIO()
    with contextlib.redirect_stdout(out):
        code = sg.run(
            "check",
            check.hits,
            check.BASELINE,
            check.ALLOW,
            check.HEADER,
            check.ADVICE,
            ["--root", root, *flags],
        )
    return code, out.getvalue()


class Reader(unittest.TestCase):
    def test_comments_and_literals_are_blank(self):
        text = 'let a = "x.borrow()"; // y.borrow()\nlet c = \'"\'; /* z.borrow() */ w<\'a>'
        blank = sg.blank(text)
        self.assertEqual(len(blank), len(text))
        self.assertNotIn("borrow", blank)
        self.assertIn("w<'a>", blank)

    def test_test_items_and_test_files_are_skipped(self):
        files = {
            "src/a.rs": (
                "fn live() { x.borrow(); }\n"
                "#[cfg(test)]\nfn helper() { x.borrow(); }\n"
                "#[cfg(test)]\nmod tests { fn t() { x.borrow_mut(); } }\n"
                '#[cfg(test)]\n#[path = "odd.rs"]\nmod odd;\n'
            ),
            "src/odd.rs": "mod deeper;\nfn t() { x.borrow(); }\n",
            "src/odd/deeper.rs": "fn t() { x.borrow(); }\n",
            "src/a_tests.rs": "fn t() { x.borrow(); }\n",
            "src/tests/b.rs": "fn t() { x.borrow(); }\n",
        }
        self.assertEqual(keys(borrows, files), ["live::x.borrow"])

    def test_a_test_only_field_or_statement_hides_nothing_after_it(self):
        files = {
            "src/a.rs": (
                "struct S {\n    #[cfg(test)]\n    probe: Probe,\n    cell: RefCell<u8>,\n}\n"
                "fn live(s: &S) {\n"
                "    #[cfg(test)]\n"
                "    COUNTS.with(|counts| {\n        counts.borrow_mut().push(1);\n    });\n"
                "    #[cfg(test)]\n    let seen = s.cell.borrow();\n"
                "    s.cell.borrow_mut();\n"
                "}\n"
                "fn later(s: &S) { s.cell.borrow(); }\n"
            )
        }
        self.assertEqual(keys(borrows, files), ["later::cell.borrow", "live::cell.borrow_mut"])

    def test_a_key_names_the_type_and_function(self):
        files = {
            "src/ui/a.rs": (
                "impl<T> Render for Panel<T> {\n"
                "    fn render(&self, done: impl Fn()) { self.cell.borrow_mut(); }\n"
                "}\n"
                "thread_local! { static X: u8 = Y.with(|y| *y.borrow()); }\n"
            )
        }
        self.assertEqual(
            keys(borrows, files), ["@a::y.borrow", "Panel::render::cell.borrow_mut"]
        )


class Ratchet(unittest.TestCase):
    FILES = {"src/a.rs": "fn f() { a.borrow(); a.borrow(); }\n", "scripts/.keep": ""}

    def test_baseline_passes_and_a_new_hit_fails(self):
        with tree(self.FILES) as root:
            self.assertEqual(run(borrows, root, "--write-baseline")[0], 0)
            self.assertEqual(run(borrows, root)[0], 0)
            with open(os.path.join(root, "src/a.rs"), "a", encoding="utf-8") as handle:
                handle.write("fn g() { b.borrow_mut(); }\nfn h() { a.borrow(); }\n")
            code, out = run(borrows, root)
            self.assertEqual(code, 1)
            self.assertIn("g::b.borrow_mut: 1 here, not in the baseline (src/a.rs:2)", out)
            self.assertIn("h::a.borrow", out)
            self.assertNotIn("f::a.borrow:", out)

    def test_a_count_that_grows_fails(self):
        with tree(self.FILES) as root:
            run(borrows, root, "--write-baseline")
            with open(os.path.join(root, "src/a.rs"), "w", encoding="utf-8") as handle:
                handle.write("fn f() { a.borrow(); a.borrow(); a.borrow(); }\n")
            code, out = run(borrows, root)
            self.assertEqual(code, 1)
            self.assertIn("f::a.borrow: 3 here, 2 in the baseline", out)

    def test_a_fixed_hit_is_a_hint_and_prune_only_shrinks(self):
        with tree(self.FILES) as root:
            run(borrows, root, "--write-baseline")
            with open(os.path.join(root, "src/a.rs"), "w", encoding="utf-8") as handle:
                handle.write("fn f() { a.borrow(); }\nfn g() { b.borrow(); }\n")
            code, out = run(borrows, root)
            self.assertEqual(code, 1)
            self.assertIn("f::a.borrow is down to 1; lower the count", out)
            self.assertEqual(run(borrows, root, "--prune")[0], 0)
            with open(os.path.join(root, borrows.BASELINE), encoding="utf-8") as handle:
                listed = [line for line in handle.read().splitlines() if not line.startswith("#")]
            self.assertEqual(listed, ["f::a.borrow 1"])
            self.assertEqual(run(borrows, root)[0], 1)

    def test_stale_lines_pass_unless_strict(self):
        with tree(self.FILES) as root:
            run(borrows, root, "--write-baseline")
            with open(os.path.join(root, "src/a.rs"), "w", encoding="utf-8") as handle:
                handle.write("fn f() {}\n")
            code, out = run(borrows, root)
            self.assertEqual(code, 0)
            self.assertIn("f::a.borrow is gone; remove the line", out)
            self.assertEqual(run(borrows, root, "--strict")[0], 1)

    def test_an_allowed_key_needs_a_reason(self):
        with tree(self.FILES) as root:
            with open(os.path.join(root, borrows.ALLOW), "w", encoding="utf-8") as handle:
                handle.write("f::a.borrow\n")
            self.assertEqual(run(borrows, root)[0], 1)
            with open(os.path.join(root, borrows.ALLOW), "w", encoding="utf-8") as handle:
                handle.write("f::a.borrow a watch channel, not a RefCell\n")
            self.assertEqual(run(borrows, root)[0], 0)


class Borrows(unittest.TestCase):
    def test_every_panicking_form_counts_and_try_borrow_does_not(self):
        files = {
            "src/a.rs": (
                "fn f() {\n"
                "    body.borrow_mut().take();\n"
                "    CACHE.with_borrow_mut(|c| c.clear());\n"
                "    let Ok(x) = cell.try_borrow_mut() else { return };\n"
                "    slot.take();\n"
                "}\n"
            )
        }
        self.assertEqual(
            keys(borrows, files), ["f::CACHE.with_borrow_mut", "f::body.borrow_mut"]
        )


class Timers(unittest.TestCase):
    def test_a_detached_timer_fails_and_a_held_one_passes(self):
        files = {
            "src/ui/a.rs": (
                "impl App {\n"
                "    fn toast(&mut self, cx: &mut Cx) {\n"
                "        cx.spawn(async move |this, cx| {\n"
                "            cx.background_executor().timer(WAIT).await;\n"
                "            this.update(cx, |this, _| this.toast = None).ok();\n"
                "        })\n"
                "        .detach();\n"
                "    }\n"
                "    fn held(&mut self, cx: &mut Cx) {\n"
                "        self.toast = Toast::Shown { _hide: cx.spawn(async move |_, cx| {\n"
                "            cx.background_executor().timer(WAIT).await;\n"
                "        }) };\n"
                "    }\n"
                "    fn once(&mut self, cx: &mut Cx) {\n"
                "        cx.spawn(async move |_, _| save().await).detach();\n"
                "    }\n"
                "    fn named(&mut self, cx: &mut Cx) {\n"
                "        let task = cx.spawn(async move |_, cx| { sleep(WAIT).await; });\n"
                "        task.detach();\n"
                "    }\n"
                "}\n"
            )
        }
        self.assertEqual(keys(timers, files), ["detached:App::named", "detached:App::toast"])

    def test_a_timer_in_a_loop_is_a_poll(self):
        files = {
            "src/ui/a.rs": (
                "fn watch(cx: &mut Cx) -> Task<()> {\n"
                "    cx.spawn(async move |cx| loop {\n"
                "        cx.background_executor().timer(POLL).await;\n"
                "        probe();\n"
                "    })\n"
                "}\n"
                "fn woken(cx: &mut Cx) -> Task<()> {\n"
                "    cx.spawn(async move |cx| loop {\n"
                "        sleep_or_wake(cx.background_executor(), &SIGNAL, seen, CEILING).await;\n"
                "    })\n"
                "}\n"
                "fn fold(items: &[u8]) { for item in items { while more() { step(); } } }\n"
            ),
            "src/ui/a_demo.rs": "fn d(cx: &mut Cx) { loop { cx.timer(T).await; } }\n",
        }
        self.assertEqual(keys(timers, files), ["poll:watch"])

    def test_a_task_that_rearms_itself_at_a_fixed_period_is_a_poll(self):
        files = {
            "src/ui/a.rs": (
                "impl App {\n"
                "    fn tick(&mut self, cx: &mut Cx) {\n"
                "        let task = cx.spawn(async move |this, cx| {\n"
                "            cx.background_executor().timer(Duration::from_millis(250)).await;\n"
                "            this.update(cx, |this, cx| this.tick(cx)).ok();\n"
                "        });\n"
                "        self.task = Some(task);\n"
                "    }\n"
                "    fn clock(&mut self, cx: &mut Cx) {\n"
                "        let task = cx.spawn(async move |this, cx| {\n"
                "            cx.background_executor().timer(TICK).await;\n"
                "            this.update(cx, |this, cx| this.clock(cx)).ok();\n"
                "        });\n"
                "        self.task = Some(task);\n"
                "    }\n"
                # Deadline-driven: the wait comes from a stored instant.
                "    fn wake(&mut self, due: Instant, cx: &mut Cx) {\n"
                "        let task = cx.spawn(async move |this, cx| {\n"
                "            let wait = due.saturating_duration_since(Instant::now());\n"
                "            cx.background_executor().timer(wait).await;\n"
                "            this.update(cx, |this, cx| this.wake(next_due(), cx)).ok();\n"
                "        });\n"
                "        self.task = Some(task);\n"
                "    }\n"
                # Another function's call after the wait is not a re-arm.
                "    fn once(&mut self, cx: &mut Cx) {\n"
                "        let task = cx.spawn(async move |this, cx| {\n"
                "            cx.background_executor().timer(Duration::from_secs(1)).await;\n"
                "            this.update(cx, |this, cx| this.finish(cx)).ok();\n"
                "        });\n"
                "        self.task = Some(task);\n"
                "    }\n"
                "}\n"
            )
        }
        self.assertEqual(
            keys(timers, files), ["poll:App::clock", "poll:App::tick"]
        )

    def test_a_deadline_elsewhere_in_the_function_does_not_exempt_a_fixed_rearm(self):
        files = {
            "src/ui/a.rs": (
                "impl App {\n"
                # A fixed re-arm in one branch beside a deadline in another.
                "    fn clock(&mut self, due: Option<Instant>, cx: &mut Cx) {\n"
                "        let wait = match due {\n"
                "            Some(due) => due.saturating_duration_since(Instant::now()),\n"
                "            None => Duration::from_millis(300),\n"
                "        };\n"
                "        let task = cx.spawn(async move |this, cx| {\n"
                "            cx.background_executor().timer(wait).await;\n"
                "            this.update(cx, |this, cx| this.clock(None, cx)).ok();\n"
                "        });\n"
                "        self.task = Some(task);\n"
                "    }\n"
                # The same through an `if` binding and a `*_wait(` call.
                "    fn idle(&mut self, held: bool, deadline: Instant, cx: &mut Cx) {\n"
                "        let wait = if held { HELD_MS } else { deadline - Instant::now() };\n"
                "        let task = cx.spawn(async move |this, cx| {\n"
                "            cx.background_executor().timer(wait).await;\n"
                "            this.update(cx, |this, cx| this.idle(held, deadline, cx)).ok();\n"
                "        });\n"
                "        self.task = Some(task);\n"
                "    }\n"
                "    fn walk(&mut self, cx: &mut Cx) {\n"
                "        let task = cx.spawn(async move |this, cx| {\n"
                "            cx.background_executor().timer(self.registry_wait()).await;\n"
                "            this.update(cx, |this, cx| this.walk(cx)).ok();\n"
                "        });\n"
                "        self.task = Some(task);\n"
                "    }\n"
                # Every branch computed from a deadline: not a poll.
                "    fn both(&mut self, due: Instant, next: Instant, cx: &mut Cx) {\n"
                "        let wait = if due < next { due.saturating_duration_since(Instant::now()) } else { next.saturating_duration_since(Instant::now()) };\n"
                "        let task = cx.spawn(async move |this, cx| {\n"
                "            cx.background_executor().timer(wait).await;\n"
                "            this.update(cx, |this, cx| this.both(due, next, cx)).ok();\n"
                "        });\n"
                "        self.task = Some(task);\n"
                "    }\n"
                "}\n"
            )
        }
        self.assertEqual(
            keys(timers, files), ["poll:App::clock", "poll:App::idle", "poll:App::walk"]
        )

    def test_a_rearm_must_hand_on_a_stored_deadline(self):
        def wake(name, delay, rearm, lets=""):
            return (
                f"    fn {name}(&mut self, due: Instant, cx: &mut Cx) {{\n"
                f"{lets}"
                "        let task = cx.spawn(async move |this, cx| {\n"
                f"            cx.background_executor().timer({delay}).await;\n"
                f"            this.update(cx, |this, cx| this.{name}({rearm}, cx)).ok();\n"
                "        });\n"
                "        self.task = Some(task);\n"
                "    }\n"
            )

        stored = "due.saturating_duration_since(Instant::now())"
        files = {
            "src/ui/a.rs": (
                "impl App {\n"
                # The delay is a deadline, but the next one is now + a period.
                + wake("fixed_next", stored, "Instant::now() + STEP")
                + wake(
                    "bound_next",
                    stored,
                    "next",
                    "        let next = Instant::now() + Duration::from_millis(100);\n",
                )
                # A `let` constant period read inside a compound expression.
                + wake(
                    "period",
                    "period.saturating_sub(started.elapsed())",
                    "due",
                    "        let period = Duration::from_secs(10);\n",
                )
                # A field that only has the word in its name.
                + wake("remaining", "self.remaining", "due")
                # A constant floor turns the wait into a period.
                + wake("floor", stored + ".max(Duration::from_secs(10))", "due")
                # Passing: the next deadline comes from the stage; a frame floor.
                + wake("stage", stored, "self.stage.next_due(Instant::now())")
                + wake("frame", stored + ".max(Duration::from_millis(16))", "due")
                + "}\n"
            )
        }
        self.assertEqual(
            keys(timers, files),
            [
                "poll:App::bound_next",
                "poll:App::fixed_next",
                "poll:App::floor",
                "poll:App::period",
                "poll:App::remaining",
            ],
        )

    def test_a_loop_that_sleeps_a_computed_wait_is_still_a_poll(self):
        files = {
            "src/ui/a.rs": (
                "async fn watch(spent: Duration) {\n"
                "    loop {\n"
                "        cx.background_executor().timer(PERIOD.saturating_sub(spent)).await;\n"
                "    }\n"
                "}\n"
            )
        }
        self.assertEqual(keys(timers, files), ["poll:watch"])


class Requests(unittest.TestCase):
    def test_purpose_checks_and_unmatched_answers_fail(self):
        files = {
            "src/connect/a.rs": (
                "impl Driver {\n"
                "    fn fetch(&mut self) {\n"
                "        if self.session.requests.has_purpose(RequestPurpose::GetThing) { return; }\n"
                "        drop(self.session.requests.take_purpose(RequestPurpose::GetThing));\n"
                "    }\n"
                "    fn keyed(&mut self, chat: ChatId) -> bool {\n"
                "        self.session.requests.has_purpose_for_chat(RequestPurpose::GetThing, chat)\n"
                "    }\n"
                "}\n"
            ),
            "src/state/a.rs": (
                "impl Session {\n"
                "    fn apply(&mut self, pending: Option<&Pending>, value: Thing) {\n"
                "        self.thing.succeed(value.clone());\n"
                "        self.thing.answered(pending.map(|p| p.id), value);\n"
                "    }\n"
                "}\n"
            ),
            "src/state/requests.rs": "impl Requests { fn any(&self) { self.has_purpose(P); } }\n",
            "src/ui/a_demo.rs": "fn seed(s: &mut Session) { s.thing.succeed(Thing); }\n",
        }
        self.assertEqual(
            keys(requests, files),
            ["purpose:Driver::fetch", "purpose:Driver::fetch", "unmatched:Session::apply"],
        )

    def test_a_counter_in_a_purpose_variant_fails(self):
        files = {
            "src/state/domains/a/purpose.rs": (
                "pub enum ChatsPurpose {\n"
                "    DeepLinkInfo { generation: u64 },\n"
                "    Labels {\n        epoch: Option<u32>,\n    },\n"
                "    Plain { request: RequestId },\n"
                "    Id(u64),\n"
                "}\n"
                "pub enum Stage { Jump { generation: u64 } }\n"
            ),
            "src/ui/a_demo.rs": "pub enum DemoPurpose { Seed { token: u8 } }\n",
        }
        self.assertEqual(
            keys(requests, files),
            ["counter:ChatsPurpose::DeepLinkInfo", "counter:ChatsPurpose::Labels"],
        )


    def test_a_purpose_with_a_counter_stands_in_for_the_request_id(self):
        files = {
            "src/state/domains/a/purpose.rs": (
                "pub enum APurpose {\n"
                "    Lookup { generation: u64 },\n"
                "    Resolve,\n"
                "    Box { token: u64 },\n"
                "    Keyed { chat_id: i64, offset: i32 },\n"
                "}\n"
            ),
            "src/state/domains/a/state.rs": "pub struct S { pub token: u64 }\n",
            # A purpose enum in a file whose name has no "purpose" in it.
            "src/feature/mod.rs": (
                "pub struct Own { pub generation: u64 }\n"
                "pub enum PickerPurpose {\n"
                "    Search { generation: u64 },\n"
                "    Tile,\n"
                "}\n"
                "pub enum Plain { Go { token: u64 } }\n"
            ),
        }
        self.assertEqual(
            keys(requests, files),
            [
                "counter:APurpose::Box",
                "counter:APurpose::Lookup",
                "counter:PickerPurpose::Search",
            ],
        )


class Enums(unittest.TestCase):
    def test_in_flight_variants_must_hold_a_request_or_a_task(self):
        files = {
            "src/state/a.rs": (
                "pub enum Load { #[default] Idle, Loading, Ready(Thing), Failed(String) }\n"
                "pub enum Gate { Idle, Asking { at: u64, chat: ChatId } }\n"
                "pub enum Good { Idle, Asking(RequestId), Sending(Flight), Running { _work: Task<()> } }\n"
                "pub struct Flight { request: RequestId, text: String }\n"
                "pub enum Side { Left, Right }\n"
                "pub enum BoxAction { Open, Closing }\n"
            ),
            "src/telegram/envelope/a.rs": "pub enum CallState { Pending, Ready }\n",
        }
        self.assertEqual(keys(enums, files), ["bare:Load::Loading", "unkeyed:Gate::Asking"])

    def test_a_u64_standing_in_for_a_request_id_fails(self):
        files = {
            "src/state/a.rs": (
                "pub enum Fetch { Idle, Loading { request: u64 } }\n"
                "pub enum Fetch2 { Idle, Waiting { chat: ChatId, serial: u64 } }\n"
                "pub enum Sound { Idle, Ready { generation: u64 } }\n"
                "pub enum Good { Idle, Loading { request: RequestId }, Ready { count: u64 } }\n"
                "pub enum UsersPurpose { Check { generation: u64 } }\n"
            ),
        }
        self.assertEqual(
            keys(enums, files),
            [
                "rawid:Fetch2::Waiting",
                "rawid:Fetch::Loading",
                "rawid:Sound::Ready",
                "unkeyed:Fetch2::Waiting",
                "unkeyed:Fetch::Loading",
            ],
        )


if __name__ == "__main__":
    unittest.main()
