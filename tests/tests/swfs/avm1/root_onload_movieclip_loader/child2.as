trace("child2 frame 1");
this.onLoad = function() {
   trace("child2 onLoad");
};
var enterFrameTraced = false;
this.onEnterFrame = function() {
   if (!enterFrameTraced) {
      enterFrameTraced = true;
      trace("child2 onEnterFrame");
   }
};
