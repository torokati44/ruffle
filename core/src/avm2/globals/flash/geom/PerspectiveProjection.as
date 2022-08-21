package flash.geom {
	public class PerspectiveProjection {

		public var fieldOfView:Number;
		public var focalLength:Number;
		public var projectionCenter:Point;

		public function PerspectiveProjection() {}

		public function toMatrix3D():Matrix3D {}

		public function toString():String {}

	}
}
