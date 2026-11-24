trace("child1 frame 1");
this.onLoad = function() {
   trace("child1 onLoad");
};
var enterFrameTraced = false;
this.onEnterFrame = function() {
   if (!enterFrameTraced) {
      enterFrameTraced = true;
      trace("child1 onEnterFrame");
   }
};
