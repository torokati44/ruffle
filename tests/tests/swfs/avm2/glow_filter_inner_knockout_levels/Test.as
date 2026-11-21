package {
	import flash.display.BitmapData;
	import flash.display.MovieClip;
	import flash.display.Shape;
	import flash.filters.GlowFilter;
	import flash.geom.ColorTransform;
	import flash.geom.Matrix;
	import flash.geom.Point;

	// Based on https://github.com/ruffle-rs/ruffle/issues/19516: an inner, knockout
	// GlowFilter on green shapes. The blur of the shape must not leave a faint glow
	// in the middle of the shape (where the blurred alpha is still full).
	public class Test extends MovieClip {
		public function Test() {
			for each (var strength:Number in [1, 3, 25]) {
				testShape("square", strength, 80);
				testShape("line", strength, 3);
			}
		}

		private static function hex(v:uint):String {
			var s:String = v.toString(16).toUpperCase();
			while (s.length < 8) {
				s = "0" + s;
			}
			return s;
		}

		private static function testShape(name:String, strength:Number, width:int):void {
			var shape:Shape = new Shape();
			shape.graphics.beginFill(0xFFFFFF);
			shape.graphics.drawRect(20, 20, width, 80);
			shape.graphics.endFill();

			var src:BitmapData = new BitmapData(120, 120, true, 0x00000000);
			// The color transform from the issue (makes the shape green).
			src.draw(shape, new Matrix(), new ColorTransform(0, 0.8984375, 0, 1, 0, 0, 0, 0));

			var dst:BitmapData = new BitmapData(120, 120, true, 0x00000000);
			dst.applyFilter(src, src.rect, new Point(0, 0), new GlowFilter(0xB3B3B3, 0.3, 8, 8, strength, 2, true, true));

			// A horizontal row through the middle of the shape, from outside to the center.
			var row:Array = [];
			var end:int = 20 + Math.min(width, 41);
			for (var x:int = 16; x < end + 1; x++) {
				row.push(hex(dst.getPixel32(x, 60)));
			}
			trace(name + " strength=" + strength + ": " + row.join(" "));
		}
	}
}
