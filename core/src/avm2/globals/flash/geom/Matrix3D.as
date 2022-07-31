package flash.geom {
	public class Matrix3D {

		var rawData : Vector.<Number>; // A Vector of 16 Numbers, where every four elements is a column of a 4x4 matrix.

		public function get position() : Vector3D {
			//A Vector3D object that holds the position, the 3D coordinate (x,y,z) of a display object within the transformation's frame of reference.
			return new Vector3D(rawData[12], rawData[13], rawData[14], rawData[15]);
		}

		public function set position(value : Vector3D) : void {
			rawData[12] = value.x;
			rawData[13] = value.y;
			rawData[14] = value.z;
			rawData[15] = value.w;
		}


		public function get determinant() : Number {
			//[read-only] A Number that determines whether a matrix is invertible.
			return
			+ rawData[0] * rawData[5] * rawData[10] * rawData[15]
			- rawData[0] * rawData[5] * rawData[11] * rawData[14]
			- rawData[0] * rawData[6] * rawData[9]  * rawData[15]
			+ rawData[0] * rawData[6] * rawData[11] * rawData[13]
			+ rawData[0] * rawData[7] * rawData[9]  * rawData[14]
			- rawData[0] * rawData[7] * rawData[10] * rawData[13]

			- rawData[1] * rawData[4] * rawData[10] * rawData[15]
			+ rawData[1] * rawData[4] * rawData[11] * rawData[14]
			+ rawData[1] * rawData[6] * rawData[8]  * rawData[15]
			- rawData[1] * rawData[6] * rawData[11] * rawData[12]
			- rawData[1] * rawData[7] * rawData[8]  * rawData[14]
			+ rawData[1] * rawData[7] * rawData[10] * rawData[12]

			+ rawData[2] * rawData[4] * rawData[9]  * rawData[15]
			- rawData[2] * rawData[4] * rawData[11] * rawData[13]
			- rawData[2] * rawData[5] * rawData[8]  * rawData[15]
			+ rawData[2] * rawData[5] * rawData[11] * rawData[12]
			+ rawData[2] * rawData[7] * rawData[8]  * rawData[13]
			- rawData[2] * rawData[7] * rawData[9]  * rawData[12]

			- rawData[3] * rawData[4] * rawData[9]  * rawData[14]
			+ rawData[3] * rawData[4] * rawData[10] * rawData[13]
			+ rawData[3] * rawData[5] * rawData[8]  * rawData[14]
			- rawData[3] * rawData[5] * rawData[10] * rawData[12]
			- rawData[3] * rawData[6] * rawData[8]  * rawData[13]
			+ rawData[3] * rawData[6] * rawData[9]  * rawData[12]
			;
		}


		public function Matrix3D(v : Vector.<Number> = null) {

		}

		public function append(lhs:Matrix3D) : void {
			//Concatenates a matrix by multiplying another Matrix3D object by the current Matrix3D object.
		}

		public function appendRotation(degrees:Number, axis:Vector3D, pivotPoint:Vector3D = null) : void {
			//Appends a rotation transformation to the current matrix.
		}

		public function appendScale(xScale:Number, yScale:Number, zScale:Number) : void {
			//Appends a scaling transformation to the current matrix.
		}

		public function appendTranslation(x:Number, y:Number, z:Number) : void {
			//Appends a translation transformation to the current matrix.
		}

		public function clone() : Matrix3D {
			//Returns a new Matrix3D object that is a clone of the current Matrix3D object.
		}

		public function copyColumnFrom(column:uint, vector3D:Vector3D) : void {
			//Copies a column from a Vector3D object to the calling Matrix3D object.
		}

		public function copyColumnTo(column:uint, vector3D:Vector3D) : void {
			//Copies a column from the calling Matrix3D object to a Vector3D object.
		}

		public function copyFrom(sourceMatrix3D:Matrix3D) : void {
			//Copies all of the matrix data from the source Matrix3D object into the calling Matrix3D object.
		}

		public function copyRawDataFrom(vector:Vector.<Number>, index:uint = 0, transpose:Boolean = false) : void {
			//Copies a Vector of Numbers into specific column-major or row-major matrix positions.
		}

		public function copyRawDataTo(vector:Vector.<Number>, index:uint = 0, transpose:Boolean = false) : void {
			//Copies specific column-major or row-major matrix positions into a Vector of Numbers.
		}

		public function copyRowFrom(row:uint, vector3D:Vector3D) : void {
			//Copies a row from a Vector3D object to the calling Matrix3D object.
		}

		public function copyRowTo(row:uint, vector3D:Vector3D) : void {
			//Copies a row from the calling Matrix3D object to a Vector3D object.
		}

		public function copyToMatrix3D(dest:Matrix3D) : void {
			//Copies all of the matrix data from the calling Matrix3D object into the destination Matrix3D object.
		}

		// TODO: can somehow use Orientation3D.EULER_ANGLES as default?
		public function decompose(orientationStyle:String = "eulerAngles") : Vector.<Vector3D> {
			//Decomposes a Matrix3D object into a translation, rotation, and scale.
		}

		public function deltaTransformVector(v:Vector3D) : Vector3D {
			//Returns a new Vector3D object that is the result of applying the inverse of the transformation to a specified Vector3D object.
		}

		public function identity() : void {
			//Sets each matrix property to a value that causes a null transformation.
		}

		public function interpolateTo(toMat:Matrix3D, percent:Number) : void {
			//Interpolates the caller's matrix towards the matrix of the toMat parameter.
		}

		public function invert() : void {
			//Inverts the current matrix.
		}

		public function pointAt(pos:Vector3D, at:Vector3D = null, up:Vector3D = null) : void {
			//Sets the current matrix as a matrix that can be used to map a 3D point to a 2D point on the screen.
		}

		public function prepend(lhs:Matrix3D) : void {
			//Prepends a matrix by multiplying the current Matrix3D object by another Matrix3D object.
		}

		public function prependRotation(degrees:Number, axis:Vector3D, pivotPoint:Vector3D = null) : void {
			//Prepends a rotation transformation to the current matrix.
		}

		public function prependScale(xScale:Number, yScale:Number, zScale:Number) : void {
			//Prepends a scaling transformation to the current matrix.
		}

		public function prependTranslation(x:Number, y:Number, z:Number) : void {
			//Prepends a translation transformation to the current matrix.
		}

		// TODO: can somehow use Orientation3D.EULER_ANGLES as default?
		public function recompose(components:Vector.<Vector3D>, orientationStyle:String = "eulerAngles") : void {
			//Recreates the current matrix by using the translation, rotation, and scale components.
		}

		public function transformVector(v:Vector3D) : Vector3D {
			//Returns a new Vector3D object that is the result of applying the transformation to a specified Vector3D object.
		}

		public function transformVectors(vin:Vector.<Number>, vout:Vector.<Number>) : void {
			//Transforms a Vector of Numbers from one space coordinate to another.
		}

		public function transpose() : void {
			//Sets the current matrix as the transpose of the original matrix.
		}

		public function toString() : String {
			//Returns a string that contains all the properties of the Matrix3D object.
		}

		public static function interpolate(thisMat:Matrix3D, toMat:Matrix3D, percent:Number) : Matrix3D {
			//Interpolates the matrix of the thisMat parameter towards the matrix of the toMat parameter.
		}


	}
}
