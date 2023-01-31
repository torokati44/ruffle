// Based on the MIT-licensed OpenFL code https://github.com/openfl/openfl/blob/develop/src/openfl/geom/PerspectiveProjection.hx

package flash.geom {
    public class PerspectiveProjection {




        public var _fieldOfView: Number;
		public var _focalLength: Number;
		public var projectionCenter: Point;


		// Getters & Setters
		public function get fieldOfView():Number
		{
			return this._fieldOfView;
		}

		public function set fieldOfView(fieldOfView:Number):void
		{
			this._fieldOfView = fieldOfView;
			this._focalLength = 250.0 * (1.0 / Math.tan(_fieldOfView * Math.PI / 180.0 * 0.5));
		}

		public function toMatrix3D():Matrix3D
		{
			if (projectionCenter == null) return null;

			var matrix3D:Matrix3D = new Matrix3D();
			var _mp = matrix3D.rawData;
			_mp[0] = this._focalLength;
			_mp[5] = this._focalLength;
			_mp[11] = 1.0;
			_mp[15] = 0;

			// matrix3D.rawData = [357.0370178222656,0,0,0,0,357.0370178222656,0,0,0,0,1,1,0,0,0,0];
			return matrix3D;
		}



    }
}