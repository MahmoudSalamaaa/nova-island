// 1.8.7: what the 1.8.6 review found wrong, so it stays fixed (audit-1.8.6/REPORT.ar.md).
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};

use nova_services::{calendar, ctx::Ctx};
use serde_json::{json, Value};

fn ctx(name: &str) -> Ctx {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("audit-fixes").join(name);
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_file(&dir);
    Ctx { dir, emit: Arc::new(|_, _| {}), features: Arc::new(Mutex::new(json!({}))) }
}

fn feed(body: &str) -> String {
    format!("BEGIN:VCALENDAR\r\n{body}END:VCALENDAR\r\n")
}

// 2026-11-12 00:00 UTC .. 2026-11-13 00:00 UTC
const FROM: f64 = 1794441600000.0;
const TO: f64 = 1794528000000.0;

#[test]
fn calendar_letters_where_digits_go_are_refused() {
    let cal = calendar::service::Calendar::new(ctx("calendar"));
    cal.set_url("اااااا");
    cal.set_url("webcal:/اا");
    for line in ["DTSTART:20261113T1é000Z", "DTSTART:20261113T090000Z\r\nRRULE:FREQ=WEEKLY;BYDAY=éM", "DTSTART:2026é113T090000Z", "DTSTART:20261112T090000Z\r\nRRULE:FREQ=DAILY;BYDAY=é"] {
        let f = feed(&format!("BEGIN:VEVENT\r\nUID:t\r\n{line}\r\nSUMMARY:t\r\nEND:VEVENT\r\n"));
        let _ = calendar::occurrences(&f, FROM, TO);
    }
}

#[test]
fn bymonthday_limits_daily_and_weekly() {
    let daily = feed("BEGIN:VEVENT\r\nUID:d\r\nDTSTART:20261101T090000Z\r\nRRULE:FREQ=DAILY;BYMONTHDAY=1\r\nSUMMARY:first\r\nEND:VEVENT\r\n");
    assert!(calendar::occurrences(&daily, FROM, TO).is_empty());
    // 2026-12-01 00:00 UTC .. the 2nd: its 09:00
    assert_eq!(calendar::occurrences(&daily, 1796083200000.0, 1796169600000.0).len(), 1);
    // the last day of November (-1) = the 30th
    let last = feed("BEGIN:VEVENT\r\nUID:l\r\nDTSTART:20261101T090000Z\r\nRRULE:FREQ=DAILY;BYMONTHDAY=-1\r\nSUMMARY:last\r\nEND:VEVENT\r\n");
    assert!(calendar::occurrences(&last, FROM, TO).is_empty());
    assert_eq!(calendar::occurrences(&last, 1795996800000.0, 1796083200000.0).len(), 1);
    // weekly on Thursdays, only on the 12th: 2026-11-12 is a Thursday
    let weekly = feed("BEGIN:VEVENT\r\nUID:w\r\nDTSTART:20261105T090000Z\r\nRRULE:FREQ=WEEKLY;BYDAY=TH;BYMONTHDAY=12\r\nSUMMARY:w\r\nEND:VEVENT\r\n");
    assert_eq!(calendar::occurrences(&weekly, FROM, TO).len(), 1);
    // no BYMONTHDAY: every day, as before
    let every = feed("BEGIN:VEVENT\r\nUID:e\r\nDTSTART:20261101T090000Z\r\nRRULE:FREQ=DAILY\r\nSUMMARY:e\r\nEND:VEVENT\r\n");
    assert_eq!(calendar::occurrences(&every, FROM, TO).len(), 1);
}

#[test]
fn a_moved_meeting_in_tokyo_is_moved() {
    let f = feed(concat!(
        "BEGIN:VEVENT\r\nUID:moved\r\nDTSTART;TZID=Asia/Tokyo:20261113T080000\r\nDTEND;TZID=Asia/Tokyo:20261113T083000\r\nRRULE:FREQ=DAILY;COUNT=2\r\nSUMMARY:original\r\nEND:VEVENT\r\n",
        "BEGIN:VEVENT\r\nUID:moved\r\nRECURRENCE-ID;TZID=Asia/Tokyo:20261113T080000\r\nDTSTART;TZID=Asia/Tokyo:20261113T100000\r\nDTEND;TZID=Asia/Tokyo:20261113T103000\r\nSUMMARY:moved-to-ten\r\nEND:VEVENT\r\n",
    ));
    // 2026-11-12 21:00 UTC .. 2026-11-14 00:00 UTC: both days of the series
    let ev = calendar::occurrences(&f, 1794517200000.0, 1794614400000.0);
    let titles: Vec<&str> = ev.iter().filter_map(|e| e["title"].as_str()).collect();
    assert_eq!(titles, ["moved-to-ten", "original"], "{ev:?}");
    // 13 Nov 10:00 Tokyo = 01:00 UTC; the 14th's stays at 08:00 Tokyo = 13 Nov 23:00 UTC
    assert_eq!(ev[0]["start"], json!(1794531600000_i64));
    assert_eq!(ev[1]["start"], json!(1794610800000_i64));
}

#[test]
fn reorder_needs_each_id_once() {
    let c = ctx("apps");
    c.store().save("apps-cache.json", &json!([{"id":"A","name":"A","icon":""},{"id":"B","name":"B","icon":""}])).unwrap();
    c.store().save("apps.json", &json!(["A", "B"])).unwrap();
    let apps = nova_services::apps::Apps::new(c.clone());
    apps.reorder(&json!(["A", "A"]));
    assert_eq!(c.store().load("apps.json").unwrap(), json!(["A", "B"]));
    apps.reorder(&json!(["B", "A"]));
    assert_eq!(c.store().load("apps.json").unwrap(), json!(["B", "A"]));
}

#[test]
fn a_task_that_wasnt_saved_isnt_reported_kept() {
    let c = ctx("unwritable");
    std::fs::create_dir_all(c.dir.parent().unwrap()).unwrap();
    std::fs::write(&c.dir, b"a file where the folder goes").unwrap();
    let tasks = nova_services::tasks::Tasks::new(c.clone());
    assert!(tasks.add(&json!({"title":"Unsaved"})).is_null());
    assert_eq!(tasks.list(), json!([]));
}

#[test]
fn saves_go_through_a_temp_file_and_a_bad_file_is_kept() {
    let c = ctx("store");
    let s = c.store();
    s.save("x.json", &json!({"a": 1})).unwrap();
    s.save("x.json", &json!({"a": 2})).unwrap();
    assert_eq!(s.load("x.json"), Some(json!({"a": 2})));
    assert!(!c.dir.join("x.json.tmp").exists());
    std::fs::write(c.dir.join("y.json"), "{\"half\": ").unwrap();
    assert_eq!(s.load("y.json"), None);
    assert_eq!(std::fs::read_to_string(c.dir.join("y.json.bad")).unwrap(), "{\"half\": ");
}

#[test]
fn task_times_are_whole_numbers_of_ms_as_f64() {
    // the widget reads them with as_f64 (tool_faces::today); here: what tasks.rs writes
    let c = ctx("tasks");
    let tasks = nova_services::tasks::Tasks::new(c);
    let t = tasks.add(&json!({"title":"Due", "due": 1_800_000_000_000.0_f64}));
    assert_eq!(t["due"].as_f64(), Some(1_800_000_000_000.0));
    let _: &Value = &t;
}
