package {
	import flash.display.BitmapData;
	import flash.display.MovieClip;
	import flash.filters.BlurFilter;
	import flash.geom.Point;
	import flash.geom.Rectangle;

	public class Test extends MovieClip {
		public function Test() {
			trace("// Part A: blurring a uniform color must not change it (tests the sum of the kernel weights)");
			testFlat();
			trace("");
			trace("// Part B: horizontal blur of a 1px line and of a 16px block (tests the kernel shape and symmetry)");
			testShape();
			trace("");
			trace("// Part C: like the 1px line in part B, but at every intensity from 1 to 255 (tests the weights and the rounding precisely)");
			testStaircase(1, [
				1.1, 1.3, 1.5, 1.7, 1.9,
				2, 2.1, 2.3, 2.5, 2.7, 2.9,
				3, 3.2, 3.5, 3.7, 3.8,
				4, 4.3, 4.7, 4.9, 5.4, 6, 6.4, 6.9, 7.4,
				8.3, 9.9, 10.5, 11.1, 12.9, 15.3,
				17.3, 23.7, 31.9, 40.3, 63.9, 100.7, 200.3
			]);
			trace("");
			trace("// Part D: like part C, but with quality 2 (tests what happens between passes)");
			testStaircase(2, [1.1, 1.5, 2, 2.5, 3, 3.2, 3.7, 4.3, 5.4, 6.9, 9.9, 15.3, 31.9, 63.9]);
		}

		// For each blur size, 255 isolated 1px lines with intensities 1 to 255
		// are drawn, far enough apart that their blurs don't overlap. They are
		// laid out in bands of 32 lines, each band 3 rows tall, read in its middle row.
		// After a horizontal blur, every pixel position around a line is a
		// staircase function of the line intensity v. For each position (offset
		// from the line), the traced numbers are the values of v at which the
		// output goes up by one, a value repeated k times if it goes up by k at
		// once, and negated if it goes down. The output at intensity v is the
		// number of entries up to v, so this is lossless. The steps show each
		// weight to about 1/65000, whether the result is floored or rounded,
		// and any intermediate quantization (as irregular steps).
		// Consecutive offsets with identical staircases are merged into a range.
		private static function testStaircase(quality:int, sizes:Array):void {
			var perBand:int = 32;
			var bands:int = Math.ceil(255 / perBand);

			for each (var size:Number in sizes) {
				var reach:int = (Math.ceil(size / 2) + 1) * quality;
				var spacing:int = 2 * reach + 2;
				var bd:BitmapData = new BitmapData(spacing * (perBand + 1), bands * 3, false, 0xFF000000);
				for (var v:int = 1; v <= 255; v++) {
					bd.fillRect(new Rectangle(lineX(v, perBand, spacing), lineBand(v, perBand) * 3, 1, 3), 0xFF000000 | (v * 0x010101));
				}
				bd.applyFilter(bd, bd.rect, new Point(0, 0), new BlurFilter(size, 0, quality));

				var runStart:int = 0;
				var runSteps:String = null;
				for (var offset:int = -reach; offset <= reach + 1; offset++) {
					var stepsText:String = null;
					if (offset <= reach) {
						var steps:Array = [];
						var previous:int = 0;
						for (v = 1; v <= 255; v++) {
							var current:int = bd.getPixel32(lineX(v, perBand, spacing) + offset, lineBand(v, perBand) * 3 + 1) & 0xFF;
							for (var k:int = previous; k < current; k++) {
								steps.push(v);
							}
							for (k = current; k < previous; k++) {
								steps.push(-v);
							}
							previous = current;
						}
						stepsText = steps.join(" ");
					}
					if (stepsText !== runSteps) {
						if (runSteps != null && runSteps != "") {
							var range:String = (runStart == offset - 1) ? String(runStart) : (runStart + ".." + (offset - 1));
							trace("staircase " + (quality == 1 ? "" : "q=" + quality + " ") + "blur=" + size + " offset=" + range + ": " + runSteps);
						}
						runStart = offset;
						runSteps = stepsText;
					}
				}
			}
		}

		private static function lineX(v:int, perBand:int, spacing:int):int {
			return ((v - 1) % perBand + 1) * spacing;
		}

		private static function lineBand(v:int, perBand:int):int {
			return int((v - 1) / perBand);
		}

		private static function hex(v:uint):String {
			var s:String = v.toString(16).toUpperCase();
			while (s.length < 8) {
				s = "0" + s;
			}
			return s;
		}

		private static function pad(v:uint):String {
			var s:String = v.toString();
			while (s.length < 3) {
				s = " " + s;
			}
			return s;
		}

		// For each blur size and quality, a uniformly filled bitmap is blurred
		// in place, and its center pixel is traced. The center is far enough
		// from the edges that only the uniform color contributes to it, so
		// with correctly normalized weights the color would come back unchanged.
		private static function testFlat():void {
			// [transparent, fill color]
			var fills:Array = [
				[false, 0xFFFFFFFF],
				[false, 0xFFB3B3B3],
				[false, 0xFF808080],
				[false, 0xFF007200],
				[true, 0xFFFFFFFF],
				[true, 0x4CB3B3B3]
			];
			var sizes:Array = [];
			for (var t:int = 11; t <= 120; t++) {
				sizes.push(t / 10);
			}
			sizes.push(12.75, 16, 16.3, 20.5, 25.25, 32, 33.7, 40);

			for (var q:int = 1; q <= 3; q++) {
				for each (var size:Number in sizes) {
					var line:String = "q=" + q + " blur=" + size + ":";
					for each (var fill:Array in fills) {
						var bd:BitmapData = new BitmapData(160, 160, fill[0], fill[1]);
						bd.applyFilter(bd, bd.rect, new Point(0, 0), new BlurFilter(size, size, q));
						line += " " + hex(bd.getPixel32(80, 80));
					}
					trace(line);
				}
			}
		}

		// A white 1px vertical line and a white 16px wide block on black,
		// blurred horizontally only, with a single pass. The traced values
		// are the blue channel of a row, spanning the whole affected area.
		// The line shows each kernel weight on its own, the edges of the block
		// show the running sum of the weights from both sides.
		private static function testShape():void {
			var sizes:Array = [1.5, 2, 2.5, 3, 3.2, 3.5, 3.7, 3.8, 4, 4.5, 5, 5.4, 6, 6.4, 6.9, 7.4, 8, 9.9, 10.5, 15.3];
			var shapes:Array = [["line", 1], ["block", 16]];

			for each (var shape:Array in shapes) {
				for each (var size:Number in sizes) {
					var bd:BitmapData = new BitmapData(128, 4, false, 0xFF000000);
					var left:int = 56;
					var width:int = shape[1];
					bd.fillRect(new Rectangle(left, 0, width, 4), 0xFFFFFFFF);
					bd.applyFilter(bd, bd.rect, new Point(0, 0), new BlurFilter(size, 0, 1));

					var reach:int = Math.ceil(size / 2) + 1;
					var line:String = shape[0] + " blur=" + size + ":";
					for (var x:int = left - reach; x < left + width + reach; x++) {
						line += " " + pad(bd.getPixel32(x, 2) & 0xFF);
					}
					trace(line);
				}
			}
		}
	}
}
