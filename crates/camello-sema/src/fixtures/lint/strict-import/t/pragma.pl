# A lowercase module whose `import` sets some other bit turns neither on.
use hinted;
print "ok\n";  #~ warning missing-strict #~ warning missing-warnings
