trace("child frame 1");
this.onLoad = function() {
   trace("child onLoad");
};
var enterFrameTraced = false;
this.onEnterFrame = function() {
   if (!enterFrameTraced) {
      enterFrameTraced = true;
      trace("child onEnterFrame");
   }
};
