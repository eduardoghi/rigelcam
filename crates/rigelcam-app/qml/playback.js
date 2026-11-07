.pragma library

// The arithmetic of the recordings, apart from the items, so tests/qml can check it. A span is
// { start, end } in seconds after midnight, end not included, and the spans of a day come sorted.

// A day as the recorder takes it, with Latin digits whatever the locale.
function isoDay(date) {
    const pad = n => String(n).padStart(2, "0")
    return date.getFullYear() + "-" + pad(date.getMonth() + 1) + "-" + pad(date.getDate())
}

// The seconds after midnight of a time like 07:30:00, or -1 if it isn't one.
function secondsOf(text) {
    const parts = /^(\d\d):(\d\d):(\d\d)$/.exec(text)
    if (!parts || parts[1] > 23 || parts[2] > 59 || parts[3] > 59)
        return -1
    return parts[1] * 3600 + parts[2] * 60 + Number(parts[3])
}

function clockText(seconds) {
    const pad = n => String(Math.floor(n)).padStart(2, "0")
    return pad(seconds / 3600) + ":" + pad(seconds / 60 % 60) + ":" + pad(seconds % 60)
}

function playbackRecorders(cameras) {
    return [...new Set(cameras.filter(camera => camera).map(camera => camera.recorder))]
}

// The day of the recorder that is behind, among the ones with a camera open.
function earliestDay(known, cameras) {
    let day = ""
    for (const recorder of playbackRecorders(cameras)) {
        const clock = known[recorder]
        if (clock && (day === "" || clock.today < day))
            day = clock.today
    }
    return day
}

// Seconds of `day` already past on every recorder, which is all of it when none is in it.
function secondsOn(day, known, cameras) {
    let seconds = 86400
    for (const recorder of playbackRecorders(cameras)) {
        const clock = known[recorder]
        if (!clock)
            continue
        if (clock.today === day)
            seconds = Math.min(seconds, clock.seconds)
        else if (clock.today < day)
            seconds = 0
    }
    return seconds
}

// The spans of every camera together, by start, for the bar.
function allSpans(spansOf) {
    return Object.values(spansOf).reduce((all, list) => all.concat(list), [])
        .sort((a, b) => a.start - b.start)
}

function recordedAt(spans, seconds) {
    return spans.some(span => seconds >= span.start && seconds < span.end)
}

// A time inside a gap goes to the next recording, and one after the last is -1.
function recordedTime(spans, seconds) {
    for (const span of spans) {
        if (seconds < span.end)
            return Math.max(seconds, span.start)
    }
    return -1
}

function lastRecordedSecond(spans) {
    return Math.max(...spans.map(span => span.end))
}

// Ten seconds back, or the end of the recording before when that lands in a gap. Going to the next
// recording, as recordedTime does, would land on the start of this same one.
function skipBackTarget(spans, position) {
    const wanted = position - 10
    let target = -1
    for (const span of spans) {
        if (span.start <= wanted)
            target = Math.max(target, Math.min(wanted, span.end - 1))
    }
    return target < 0 ? 0 : target
}

// Where a day starts playing. Today goes to the last minute recorded, but not before the start of
// the last recording, and another day to its first recording.
function startOfDay(spans, isToday) {
    if (!isToday)
        return spans[0].start
    return Math.max(spans[spans.length - 1].start, lastRecordedSecond(spans) - 60)
}

// The time on the clock after a stream opened at `openedAt` played `elapsed` seconds. The stream
// counts only recorded time, so this walks the recordings of the day jumping over the gaps.
function timeAfter(spans, openedAt, elapsed) {
    let time = openedAt
    let left = elapsed
    for (const span of spans) {
        if (span.end <= time)
            continue
        time = Math.max(time, span.start)
        if (left <= span.end - time)
            return time + left
        left -= span.end - time
        time = span.end
    }
    return time
}

// The events of the day, any recording that isn't continuous, as { start, end }. The recorder splits a long event into files, so
// one that starts where the last ended, give or take a second, is the same event.
function eventRuns(spans) {
    const runs = []
    for (const span of spans) {
        if (!span.event)
            continue
        const last = runs[runs.length - 1]
        if (last && span.start <= last.end + 1)
            last.end = Math.max(last.end, span.end)
        else
            runs.push({ start: span.start, end: span.end })
    }
    return runs
}

// The start of the first event after `position`, or -1.
function nextEvent(spans, position) {
    const run = eventRuns(spans).find(run => run.start > position)
    return run ? run.start : -1
}

// The start of the event before. Within 3 s of the start of one, it goes to the one before that,
// so pressing it again right after a jump keeps going back. The 3 s are of the clock on the wall,
// which at 8x is 24 s of the recording.
function previousEvent(spans, position, rate) {
    const runs = eventRuns(spans).filter(run => run.start < position - 3 * (rate || 1))
    return runs.length > 0 ? runs[runs.length - 1].start : -1
}
