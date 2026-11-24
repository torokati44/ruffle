trace("frame 1 start");
_root.onLoad = function() {
   trace("_root.onLoad");
};
var enterFrames = 0;
_root.onEnterFrame = function() {
   enterFrames++;
   if (enterFrames <= 2) {
      trace("onEnterFrame");
   }
};
trace("frame 1 end");
