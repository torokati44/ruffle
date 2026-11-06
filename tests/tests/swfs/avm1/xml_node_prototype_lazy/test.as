// Flash Player doesn't define the properties of XMLNode on its prototype up
// front, but when an XMLNode is created, on the prototype it gets, at most
// once per prototype object. This includes XMLNodes created by XML methods,
// but not without `new`, or without a node type and value.
//
// XML.prototype is an XMLNode, so the original XMLNode.prototype already
// gets them at startup, and doesn't get them again.//
// The properties are listed with for-in on a fresh `{}` prototype, in the
// order Flash Player enumerates them.
var UNIMPLEMENTED = {};
function own(o) {
    var r = [];
    for (var k in o) {
        if (o.hasOwnProperty(k) && !UNIMPLEMENTED[k]) {
            r.push(k);
        }
    }
    return r.join(",");
}
function count(o) {
    var n = 0;
    for (var k in o) {
        if (o.hasOwnProperty(k) && !UNIMPLEMENTED[k]) {
            n++;
        }
    }
    return n;
}
// Calls `make` with a fresh prototype for `C`, and lists what it got.
function check(label, C, make, full) {
    var orig = C.prototype;
    var fresh = {};
    C.prototype = fresh;
    var o = make(fresh);
    C.prototype = orig;
    var proto = (o.__proto__ == fresh) ? "" : " (__proto__ is not the fresh prototype)";
    trace(label + ": " + (full ? own(fresh) : count(fresh)) + proto);
}
// Calls `C` as a method of an object with the prototype `proto`, without `new`.
function callWithout(C, proto, a0, a1) {
    var o = {};
    o.__proto__ = proto;
    o.f = C;
    o.f(a0, a1);
    return o;
}
// A subclass of `C`, whose constructor calls super(a0, a1).
function checkSuper(label, C, a0, a1) {
    var orig = C.prototype;
    var cp = {};
    C.prototype = cp;
    var sp = {};
    sp.__proto__ = cp;
    sp.__constructor__ = C;
    ASSetPropFlags(sp, "__constructor__", 1);
    var Sub = function() {
        super(a0, a1);
    };
    Sub.prototype = sp;
    new Sub();
    C.prototype = orig;
    trace("super() in a subclass: " + count(sp) + " on its prototype, " + count(cp) + " on " + label + ".prototype");
}
// Creates an instance twice with the same prototype, deleting `name` in between.
function checkOnce(C, name, make) {
    var orig = C.prototype;
    var p = {};
    C.prototype = p;
    make();
    var before = p.hasOwnProperty(name);
    delete p[name];
    make();
    C.prototype = orig;
    trace(name + " deleted, created again: " + before + " -> " + p.hasOwnProperty(name));
}

trace("startup, XMLNode.prototype.nodeName: " + XMLNode.prototype.hasOwnProperty("nodeName"));
var doc = new XML("<a/>");
check("new XMLNode(1, \"a\")", XMLNode, function() { return new XMLNode(1, "a"); }, true);
check("new XMLNode(null, \"a\")", XMLNode, function() { return new XMLNode(null, "a"); });
check("new XMLNode()", XMLNode, function() { return new XMLNode(); });
check("new XMLNode(1)", XMLNode, function() { return new XMLNode(1); });
check("new XMLNode(1, undefined)", XMLNode, function() { return new XMLNode(1, undefined); });
check("new XMLNode(undefined, \"a\")", XMLNode, function() { return new XMLNode(undefined, "a"); });
check("XMLNode(1, \"a\") without new", XMLNode, function(p) { return callWithout(XMLNode, p, 1, "a"); });
check("createElement", XMLNode, function() { return doc.createElement("b"); });
check("createTextNode", XMLNode, function() { return doc.createTextNode("b"); });
check("parseXML", XMLNode, function() { doc.parseXML("<c/>"); return doc.firstChild; });
checkSuper("XMLNode", XMLNode, 1, "a");
checkOnce(XMLNode, "nodeName", function() { new XMLNode(1, "a"); });
delete XMLNode.prototype.nodeName;
new XMLNode(1, "a");
trace("original XMLNode.prototype, nodeName deleted, created again: " + XMLNode.prototype.hasOwnProperty("nodeName"));
trace("done");
