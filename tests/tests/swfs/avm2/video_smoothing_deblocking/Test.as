// Test the `smoothing` and `deblocking` properties of `Video`.
//
// Their initial values come from the `DefineVideoStream` tag, or are `false`
// and 0 for videos created with `new Video()`. `deblocking` is clamped to 0..7
// after the usual `int` coercion.
//
// test.swf is `avm2/netstream_play_stop_replay/test.swf` with this class
// imported using JPEXS FFDec, then decompressed, with a `DefineVideoStream`
// tag (flags 0x0b: smoothing, deblocking 5) and a `PlaceObject2` tag placing
// it as `the_video` added to the end of frame 1.
package {
	import flash.display.Sprite;
	import flash.events.Event;
	import flash.media.Video;
	import flash.net.NetConnection;
	import flash.net.NetStream;

	public class Test extends Sprite {
		public var the_video:Video;

		public function Test() {
			show("new Video()", new Video());
			show("new Video(160, 120)", new Video(160, 120));
			addEventListener(Event.ENTER_FRAME, onEnterFrame);
		}

		private function show(label:String, v:Video):void {
			trace(label + ": smoothing=" + v.smoothing + " deblocking=" + v.deblocking);
		}

		private function onEnterFrame(event:Event):void {
			removeEventListener(Event.ENTER_FRAME, onEnterFrame);
			show("the_video", the_video);

			var nc:NetConnection = new NetConnection();
			nc.connect(null);
			var ns:NetStream = new NetStream(nc);
			the_video.attachNetStream(ns);
			show("after attachNetStream", the_video);

			var o:* = the_video;
			var i:int;
			var sv:Array = [true, false, 1, 0, 2, -1, "false", "", "true", null, undefined, NaN];
			for (i = 0; i < sv.length; i++) {
				o.smoothing = sv[i];
				show("smoothing = " + sv[i] + " (" + typeof sv[i] + ")", the_video);
			}
			var dv:Array = [1, 2, 3, 4, 5, 6, 7, 8, 100, -1, -2, 2.9, -2.9, "3", "abc", true, null, undefined, NaN, Infinity, -Infinity, 2147483647, 2147483647.9, 2147483648, -2147483648, -2147483649, 4294967298, 0];
			for (i = 0; i < dv.length; i++) {
				o.deblocking = dv[i];
				show("deblocking = " + dv[i] + " (" + typeof dv[i] + ")", the_video);
			}
			trace("done");
		}
	}
}
