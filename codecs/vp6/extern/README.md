The files in these folder are verbatim copies from the libav project.
One exception is the BSF (bitstream filter) list, as only the `null`
one is necessary, all the others were removed from the registry.
Only the files needed for decoding VP6 video and converting it to RGBA
data are included. The subset was selected mostly by trial and error,
starting from the vp6 decoder definition, and adding the missing ones
as the compiler complained about them being missing, one by one.
The (now static) config.h files are tailored to make it as standalone
and portable as possible. There are two different configs (for Desktop
and for Web), but at the moment the differences are tiny.
The static configs also disable any kind of platform-specific bits
(like assembly code for each architecture), but the simplified build
setup justifies this in my opinion.