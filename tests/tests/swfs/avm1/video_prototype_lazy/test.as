// Flash Player only defines width, height, smoothing and deblocking on
// Video.prototype when the first Video display object is created. They are
// defined on the prototype that this Video gets, and never again afterwards.
function has(o)
{
   return "width=" + o.hasOwnProperty("width") + " height=" + o.hasOwnProperty("height") + " smoothing=" + o.hasOwnProperty("smoothing") + " deblocking=" + o.hasOwnProperty("deblocking");
}
if(this.started == undefined)
{
   this.started = true;
   var orig = Video.prototype;
   trace("frame 1, original prototype: " + has(orig));
   var nv = new Video();
   trace("after new Video(): " + has(orig));
   var repl = {marker: 1};
   repl.width = 5;
   Video.prototype = repl;
   trace("replaced prototype: " + has(repl) + " width=" + repl.width);
   this.onEnterFrame = function()
   {
      if(this.v3 != undefined)
      {
         trace("second Video placed: " + has(repl) + " v3.smoothing=" + this.v3.smoothing);
         delete this.onEnterFrame;
         stop();
      }
      else if(this.v2 != undefined && this.sawV2 == undefined)
      {
         this.sawV2 = true;
         trace("first Video placed, original prototype: " + has(orig));
         trace("first Video placed, replaced prototype: " + has(repl) + " width=" + repl.width);
         trace("v2.__proto__ == repl: " + (this.v2.__proto__ == repl));
         trace("v2.width=" + this.v2.width + " v2.smoothing=" + this.v2.smoothing + " v2.deblocking=" + this.v2.deblocking + " v2.marker=" + this.v2.marker);
         var e = [];
         for(var k in repl)
         {
            e.push(k);
         }
         e.sort();
         trace("enumerable: " + e.join(","));
         delete repl.width;
         delete repl.smoothing;
         trace("after delete: " + has(repl));
      }
   };
}
