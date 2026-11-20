# The empty list means `import` is never called.
use My::Script ();
print "ok\n";  #~ warning missing-strict #~ warning missing-warnings
