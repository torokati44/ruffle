trace("initial root frame 1 start");
this.onLoad = function() {
   trace("initial root onLoad");
};
var enterFrameTraced = false;
this.onEnterFrame = function() {
   if (!enterFrameTraced) {
      enterFrameTraced = true;
      trace("initial root onEnterFrame");
   }
};
loadMovieNum("child.swf", 0);
trace("initial root frame 1 end");
