import QtQuick
import QtTest
import "../../qml/playback.js" as Playback

// Run by `qmltestrunner -input crates/rigelcam-app/tests/qml`, which CI does too.
TestCase {
    name: "Playback"

    // 01:00 to 01:10 and 02:00 to 02:30.
    readonly property var spans: [{ start: 3600, end: 4200 }, { start: 7200, end: 9000 }]

    function test_days_and_times_use_latin_digits() {
        compare(Playback.isoDay(new Date(2026, 0, 5)), "2026-01-05")
        compare(Playback.clockText(3661), "01:01:01")
        compare(Playback.clockText(86399), "23:59:59")
    }

    function test_only_a_valid_time_is_read() {
        compare(Playback.secondsOf("07:30:15"), 27015)
        compare(Playback.secondsOf("24:00:00"), -1)
        compare(Playback.secondsOf("07:60:00"), -1)
        compare(Playback.secondsOf("7:30:00"), -1)
        compare(Playback.secondsOf(""), -1)
    }

    function test_the_recorder_behind_sets_the_day() {
        const cameras = [{ recorder: 1 }, null, { recorder: 2 }, { recorder: 1 }]
        const clocks = {
            1: { today: "2026-11-06", seconds: 30 },
            2: { today: "2026-11-05", seconds: 86390 },
        }
        compare(Playback.playbackRecorders(cameras), [1, 2])
        compare(Playback.earliestDay(clocks, cameras), "2026-11-05")
        // The one still in the 5th has 86390 s of it, the one in the 6th has all of it.
        compare(Playback.secondsOn("2026-11-05", clocks, cameras), 86390)
        // The 6th has barely started on one of them and not at all on the other.
        compare(Playback.secondsOn("2026-11-06", clocks, cameras), 0)
        compare(Playback.secondsOn("2026-11-04", clocks, cameras), 86400)
    }

    function test_a_recorder_without_a_clock_yet_doesnt_count() {
        const cameras = [{ recorder: 1 }, { recorder: 3 }]
        const clocks = { 1: { today: "2026-11-06", seconds: 100 } }
        compare(Playback.earliestDay(clocks, cameras), "2026-11-06")
        compare(Playback.secondsOn("2026-11-06", clocks, cameras), 100)
        compare(Playback.earliestDay({}, cameras), "")
    }

    function test_the_spans_of_all_cameras_come_by_start() {
        const all = Playback.allSpans({ "1:2": [{ start: 50, end: 60 }], "1:3": [{ start: 10, end: 20 }] })
        compare(all.map(span => span.start), [10, 50])
    }

    function test_a_time_in_a_gap_goes_to_the_next_recording() {
        compare(Playback.recordedTime(spans, 3700), 3700)
        compare(Playback.recordedTime(spans, 0), 3600)
        compare(Playback.recordedTime(spans, 5000), 7200)
        compare(Playback.recordedTime(spans, 9000), -1)
        verify(Playback.recordedAt(spans, 3600))
        verify(!Playback.recordedAt(spans, 4200))
        compare(Playback.lastRecordedSecond(spans), 9000)
    }

    function test_skipping_back_from_the_start_of_a_recording_lands_in_the_one_before() {
        compare(Playback.skipBackTarget(spans, 7300), 7290)
        compare(Playback.skipBackTarget(spans, 7205), 4199)
        compare(Playback.skipBackTarget(spans, 3605), 0)
    }

    function test_today_starts_near_the_end_and_other_days_at_the_start() {
        compare(Playback.startOfDay(spans, false), 3600)
        compare(Playback.startOfDay(spans, true), 8940)
        // A last recording shorter than a minute plays from its start.
        compare(Playback.startOfDay([{ start: 0, end: 100 }, { start: 500, end: 530 }], true), 500)
    }

    function test_elapsed_time_jumps_over_the_gaps() {
        compare(Playback.timeAfter(spans, 3600, 0), 3600)
        compare(Playback.timeAfter(spans, 3600, 600), 4200)
        compare(Playback.timeAfter(spans, 3600, 601), 7201)
        compare(Playback.timeAfter(spans, 4000, 300), 7300)
        // Past everything recorded it stays at the end.
        compare(Playback.timeAfter(spans, 3600, 99999), 9000)
    }
}
