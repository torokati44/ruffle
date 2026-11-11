package {
import flash.display.Sprite;
import flash.events.Event;
import flash.events.TimerEvent;
import flash.text.TextField;
import flash.text.TextFormat;
import flash.utils.Timer;
import flash.utils.getTimer;
import flash.utils.setInterval;
import flash.utils.setTimeout;

// Checks how often 1 ms timers run when the frame rate is much lower than
// the rate at which Flash Player checks timers.
//
// The traced part only reports whether each timer ran several times per frame,
// because the exact number depends on how often the player checks timers
// (about every 10 ms on Linux and every 15.6 ms on Windows).
//
// After that, the SWF keeps running and shows the number of runs per second on
// the stage, e.g. to see how it changes when the browser tab is hidden.
[SWF(frameRate="5", width="400", height="400")]
public class Test extends Sprite {
    private static const DELAY: int = 1;
    private static const FRAMES: int = 5;
    private static const MANY: int = 8;
    private static const HISTORY: int = 20;

    private var frame: int = 0;

    private var intervalRuns: int = 0;
    private var timerRuns: int = 0;
    private var timeoutRuns: int = 0;
    private var frameRuns: int = 0;

    private var second: int = -1;
    private var secondStart: int = 0;
    private var secondIntervalRuns: int = 0;
    private var secondTimerRuns: int = 0;
    private var secondTimeoutRuns: int = 0;
    private var secondFrameRuns: int = 0;
    private var history: Array = [];
    private var text: TextField;

    public function Test() {
        text = new TextField();
        text.width = 400;
        text.height = 400;
        text.defaultTextFormat = new TextFormat("_typewriter", 12);
        addChild(text);

        stage.addEventListener(Event.ENTER_FRAME, onEnterFrame);
    }

    private function onInterval(): void {
        intervalRuns++;
        secondIntervalRuns++;
    }

    private function onTimer(e: TimerEvent): void {
        timerRuns++;
        secondTimerRuns++;
    }

    private function onTimeout(): void {
        timeoutRuns++;
        secondTimeoutRuns++;
        setTimeout(onTimeout, DELAY);
    }

    private static function describe(runs: int): String {
        return runs >= MANY ? MANY + " or more" : String(runs);
    }

    private function onEnterFrame(e: Event): void {
        if (frame == 0) {
            setInterval(onInterval, DELAY);
            var timer: Timer = new Timer(DELAY);
            timer.addEventListener(TimerEvent.TIMER, onTimer);
            timer.start();
            setTimeout(onTimeout, DELAY);
            secondStart = getTimer();
            second = 0;
        } else if (frame <= FRAMES) {
            trace("frame " + frame
                + ": setInterval " + describe(intervalRuns)
                + ", Timer " + describe(timerRuns)
                + ", setTimeout " + describe(timeoutRuns));
            if (frame == FRAMES) {
                trace("Done");
            }
        }
        frame++;
        intervalRuns = 0;
        timerRuns = 0;
        timeoutRuns = 0;

        secondFrameRuns++;
        updateText();
    }

    private function updateText(): void {
        var now: int = getTimer();
        if (now - secondStart < 1000) {
            return;
        }
        var elapsed: int = now - secondStart;
        history.unshift(pad(String(second), 4)
            + pad(perSecond(secondFrameRuns, elapsed), 7)
            + pad(perSecond(secondIntervalRuns, elapsed), 13)
            + pad(perSecond(secondTimerRuns, elapsed), 8)
            + pad(perSecond(secondTimeoutRuns, elapsed), 13));
        if (history.length > HISTORY) {
            history.pop();
        }
        second += Math.round(elapsed / 1000);
        secondStart = now;
        secondFrameRuns = 0;
        secondIntervalRuns = 0;
        secondTimerRuns = 0;
        secondTimeoutRuns = 0;

        text.text = "Runs per second, frame rate " + stage.frameRate
            + ", delay " + DELAY + " ms\n\n"
            + pad("s", 4) + pad("frames", 7) + pad("setInterval", 13)
            + pad("Timer", 8) + pad("setTimeout", 13) + "\n"
            + history.join("\n");
    }

    private static function perSecond(runs: int, elapsed: int): String {
        return String(Math.round(runs * 1000 / elapsed));
    }

    private static function pad(s: String, width: int): String {
        while (s.length < width) {
            s = " " + s;
        }
        return s;
    }
}
}
