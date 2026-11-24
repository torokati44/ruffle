this.onLoad = function() {
   trace("initial root onLoad");
};

var step = 0;
this.onEnterFrame = function() {
   step++;
   if (step == 3) {
      trace("calling _root.loadMovie");
      _root.loadMovie("child.swf");
   }
};
