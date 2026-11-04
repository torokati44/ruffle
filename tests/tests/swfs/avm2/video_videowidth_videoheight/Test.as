// Test the `videoWidth` and `videoHeight` properties of `Video`.
//
// `videoWidth` and `videoHeight` are the size of the decoded video stream,
// while `width` and `height` are the on-stage bounds of the `Video`.
//
// test.swf is `avm2/netstream_play_stop_replay/test.swf` with this class
// imported using JPEXS FFDec. test.flv is a 320x233 H.263 video.
package {
	import flash.display.Sprite;
	import flash.events.Event;
	import flash.media.Video;
	import flash.net.NetConnection;
	import flash.net.NetStream;

	public class Test extends Sprite {
		private var video:Video;
		private var nc:NetConnection;
		private var ns:NetStream;
		private var lastSize:String = "0x0";
		private var frames:int = 0;

		public function Test() {
			var defaultVideo:Video = new Video();
			trace("new Video(): videoWidth=" + defaultVideo.videoWidth + " videoHeight=" + defaultVideo.videoHeight + " width=" + defaultVideo.width + " height=" + defaultVideo.height);

			video = new Video(160, 120);
			addChild(video);
			sz("unattached");

			nc = new NetConnection();
			nc.connect(null);
			ns = new NetStream(nc);
			ns.client = {
				onMetaData: function(info:Object):void {}
			};
			video.attachNetStream(ns);
			sz("after attachNetStream");
			ns.play("test.flv");
			sz("after play");

			addEventListener(Event.ENTER_FRAME, onEnterFrame);
		}

		private function sz(label:String):void {
			trace(label + ": videoWidth=" + video.videoWidth + " videoHeight=" + video.videoHeight + " width=" + video.width + " height=" + video.height);
		}

		private function onEnterFrame(event:Event):void {
			frames++;
			var cur:String = video.videoWidth + "x" + video.videoHeight;
			if (cur != lastSize) {
				sz("size changed");
				lastSize = cur;
			}
			if (frames == 60) {
				sz("before scaling");
				video.width = 640;
				video.height = 360;
				sz("after width=640 height=360");
				video.scaleX = 0.5;
				video.scaleY = 2;
				sz("after scaleX=0.5 scaleY=2");
				video.rotation = 90;
				sz("after rotation=90");
				video.rotation = 0;
				video.scaleX = 1;
				video.scaleY = 1;
				sz("after reset");

				// Read-only: assignments throw.
				try {
					video["videoWidth"] = 99;
				} catch (e:Error) {
					trace("assigning videoWidth: " + e);
				}
				try {
					video["videoHeight"] = 77;
				} catch (e:Error) {
					trace("assigning videoHeight: " + e);
				}
				sz("after assigning");

				video.clear();
				sz("after clear");
				ns.close();
				sz("after ns.close");
				video.attachNetStream(null);
				sz("after attachNetStream(null)");
				trace("done");
				removeEventListener(Event.ENTER_FRAME, onEnterFrame);
			}
		}
	}
}
