package flash.media {
    import __ruffle__.stub_method;

    import flash.display.DisplayObject
    import flash.net.NetStream

    [Ruffle(InstanceAllocator)]
    public class Video extends DisplayObject {
        public function Video(width:int = 320, height:int = 240) {
            this.init(width, height);
        }

        private native function init(width:int, height:int);

        public native function get deblocking():int;

        public native function set deblocking(value:int):void;

        public native function get smoothing():Boolean;

        public native function set smoothing(value:Boolean):void;

        public native function get videoWidth():int;

        public native function get videoHeight():int;

        public native function attachNetStream(netStream:NetStream):void;

        public function clear():void {
            stub_method("flash.media.Video", "clear");
        }
    }
}
