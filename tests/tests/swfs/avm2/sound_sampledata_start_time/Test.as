package {
    import flash.display.Sprite;
    import flash.events.Event;
    import flash.events.SampleDataEvent;
    import flash.media.Sound;
    import flash.media.SoundChannel;
    import flash.utils.ByteArray;

    // Where the `startTime` given to `Sound.play()` puts the position of a
    // channel playing generated samples.
    public class Test extends Sprite {
        public function Test() {
            playFrom("0 ms, one block", 0, [1000], function():void {
                playFrom("500 ms, one block", 500, [1000], function():void {
                    playFrom("500 ms, two blocks", 500, [2048, 1000], function():void {
                        playFrom("500 ms, big blocks", 500, [8192, 8192, 1000], function():void {
                            trace("done");
                        });
                    });
                });
            });
        }

        private static function writeSamples(data:ByteArray, count:int):void {
            for (var i:int = 0; i < count; i++) {
                data.writeFloat(0.25);
                data.writeFloat(-0.25);
            }
        }

        // The listener writes `blocks` samples on each request in turn, the
        // last one fewer than 2048, which ends the sound.
        private function playFrom(name:String, startTime:Number, blocks:Array, next:Function):void {
            var sound:Sound = new Sound();
            var requests:int = 0;
            var inPlay:Boolean = true;
            sound.addEventListener(SampleDataEvent.SAMPLE_DATA, function(e:SampleDataEvent):void {
                trace(name + ": sampleData, position " + e.position + ", in play() " + inPlay);
                writeSamples(e.data, blocks[requests++]);
            });
            var channel:SoundChannel = sound.play(startTime);
            inPlay = false;
            trace(name + ": after play(), channel.position " + channel.position.toFixed(2));
            channel.addEventListener(Event.SOUND_COMPLETE, function(e:Event):void {
                trace(name + ": soundComplete, channel.position " + channel.position.toFixed(2));
                next();
            });
        }
    }
}
