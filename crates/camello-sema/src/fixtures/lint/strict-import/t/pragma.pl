# A lowercase pragma turns on neither, whatever it does to `$^H`.
use hinted;
print "ok\n";  #~ warning missing-strict #~ warning missing-warnings
