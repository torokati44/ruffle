this.onLoad = function() {
   trace("initial root onLoad");
};

var listener = {};
listener.onLoadStart = function(target) {
   trace("listener onLoadStart " + target);
};
listener.onLoadComplete = function(target) {
   trace("listener onLoadComplete " + target);
};
listener.onLoadInit = function(target) {
   trace("listener onLoadInit " + target);
};
var mcl = new MovieClipLoader();
mcl.addListener(listener);

var step = 0;
this.onEnterFrame = function() {
   step++;
   if (step == 3) {
      trace("calling loadClip child1.swf into level 1");
      mcl.loadClip("child1.swf", 1);
   } else if (step == 10) {
      trace("calling loadClip child2.swf into level 0");
      mcl.loadClip("child2.swf", 0);
   }
};
