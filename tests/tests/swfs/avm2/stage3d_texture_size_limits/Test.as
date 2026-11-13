package {
    import flash.display.Sprite;
    import flash.display.Stage3D;
    import flash.display3D.Context3D;
    import flash.display3D.Context3DRenderMode;
    import flash.display3D.textures.TextureBase;
    import flash.events.ErrorEvent;
    import flash.events.Event;

    public class Test extends Sprite {
        // One Stage3D per profile, requested one after another so that the
        // output order is deterministic. Only profiles that Flash Player could
        // create when generating the expected output are included.
        private static const PROFILES:Array = [
            "baseline",
            "baselineConstrained"
        ];

        private static const TEXTURE_SIZES:Array = [
            [0, 0], [0, 64], [64, 0], [-1, 64], [64, -1],
            [3, 64], [64, 3], [100, 100],
            [1, 1], [1, 2048], [2048, 1], [2048, 2048],
            [1, 4096], [4096, 1], [2048, 4096], [4096, 4096],
            [1, 8192], [8192, 1], [8192, 8192], [16384, 16384],
            [3000, 3000], [4097, 4097], [4097, 64], [6000, 8192]
        ];

        private static const CUBE_SIZES:Array = [
            0, -1, 3, 100, 1, 512, 1024, 1000, 2048, 4096, 16384, 3000
        ];

        private static const RECTANGLE_SIZES:Array = [
            [0, 0], [0, 64], [64, 0], [-1, 64],
            [3, 5], [100, 100],
            [2048, 2048], [2049, 1], [1, 2049], [4096, 4096],
            [4097, 1], [1, 4097], [8192, 8192], [16384, 16384]
        ];

        private var index:int = 0;

        public function Test() {
            requestNext();
        }

        private function requestNext():void {
            if (index >= PROFILES.length) {
                trace("Done");
                return;
            }
            var stage3D:Stage3D = stage.stage3Ds[index];
            stage3D.addEventListener(Event.CONTEXT3D_CREATE, contextCreated);
            stage3D.addEventListener(ErrorEvent.ERROR, contextError);
            stage3D.requestContext3D(Context3DRenderMode.AUTO, PROFILES[index]);
        }

        private function contextError(event:ErrorEvent):void {
            trace("Profile " + PROFILES[index] + ": context creation failed: " + event.errorID + " " + event.text);
            index++;
            requestNext();
        }

        private function contextCreated(event:Event):void {
            var stage3D:Stage3D = event.target as Stage3D;
            stage3D.removeEventListener(Event.CONTEXT3D_CREATE, contextCreated);
            stage3D.removeEventListener(ErrorEvent.ERROR, contextError);
            var context:Context3D = stage3D.context3D;
            trace("Profile " + PROFILES[index] + " (reported: " + context.profile + ")");

            var size:Array;
            for each (size in TEXTURE_SIZES) {
                attempt("createTexture(" + size[0] + ", " + size[1] + ")", function():TextureBase {
                    return context.createTexture(size[0], size[1], "bgra", false);
                });
            }
            for each (var cubeSize:int in CUBE_SIZES) {
                attempt("createCubeTexture(" + cubeSize + ")", function():TextureBase {
                    return context.createCubeTexture(cubeSize, "bgra", false);
                });
            }
            for each (size in RECTANGLE_SIZES) {
                attempt("createRectangleTexture(" + size[0] + ", " + size[1] + ")", function():TextureBase {
                    return context.createRectangleTexture(size[0], size[1], "bgra", false);
                });
            }

            context.dispose(false);
            index++;
            requestNext();
        }

        private static function attempt(label:String, create:Function):void {
            try {
                var texture:TextureBase = create();
                trace(label + ": ok");
                texture.dispose();
            } catch (e:Error) {
                trace(label + ": " + e);
            }
        }
    }
}
