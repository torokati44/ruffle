// Test the `smoothing` and `deblocking` properties of `Video`.
//
// Their initial values come from the `DefineVideoStream` tag. `smoothing` is
// coerced to a boolean. `deblocking` is truncated to an integer and clamped
// to 0..7, and values outside of the i32 range become 0.
//
// This is the SWF version 6 variant of `video_smoothing_deblocking`, see there.

function show(label)
{
   trace(label + ": smoothing=" + the_video.smoothing + " (" + typeof the_video.smoothing + ") deblocking=" + the_video.deblocking + " (" + typeof the_video.deblocking + ")");
}
show("initial");
trace("proto.hasOwnProperty(smoothing): " + Video.prototype.hasOwnProperty("smoothing") + " deblocking: " + Video.prototype.hasOwnProperty("deblocking"));
trace("inst.hasOwnProperty(smoothing): " + the_video.hasOwnProperty("smoothing") + " deblocking: " + the_video.hasOwnProperty("deblocking"));
var enumerated = [];
for(var k in the_video)
{
   if(k == "smoothing" || k == "deblocking" || k == "width" || k == "height" || k == "attachVideo" || k == "clear")
   {
      enumerated.push(k);
   }
}
enumerated.sort();
trace("enumerated: " + enumerated.join(","));
var nc = new NetConnection();
nc.connect(null);
var ns = new NetStream(nc);
the_video.attachVideo(ns);
show("after attachVideo");
var sv = [true,false,1,0,2,-1,"false","","true",null,undefined,NaN];
var i = 0;
while(i < sv.length)
{
   the_video.smoothing = sv[i];
   show("smoothing = " + sv[i] + " (" + typeof sv[i] + ")");
   i++;
}
var dv = [1,2,3,4,5,6,7,8,100,-1,-2,2.9,-2.9,"3","abc",true,null,undefined,NaN,Infinity,-Infinity,2147483647,2147483647.9,2147483648,-2147483648,-2147483649,4294967298,0];
i = 0;
while(i < dv.length)
{
   the_video.deblocking = dv[i];
   show("deblocking = " + dv[i] + " (" + typeof dv[i] + ")");
   i++;
}
the_video.smoothing = true;
the_video.deblocking = 3;
delete the_video.smoothing;
delete the_video.deblocking;
show("after delete");
the_video.attachVideo(null);
show("after attachVideo(null)");
var nv = new Video();
var nvKeys = [];
for(var k in nv)
{
   nvKeys.push(k);
}
nvKeys.sort();
trace("new Video() enumerated: " + nvKeys.join(","));
nv.smoothing = true;
nv.deblocking = 3;
trace("new Video() after assigning: smoothing=" + nv.smoothing + " deblocking=" + nv.deblocking + " own=" + nv.hasOwnProperty("smoothing"));
delete Video.prototype.smoothing;
delete Video.prototype.deblocking;
show("after deleting from Video.prototype");
trace("done");
