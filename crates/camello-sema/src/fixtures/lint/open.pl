use strict;
use warnings;

my $path = 'x';
open(FH, "< $path");                  #~ warning two-arg-open #~ warning bareword-filehandle
open(my $fh, $path);                  #~ warning two-arg-open
open FILE, '<', $path;                #~ warning bareword-filehandle
open(my $in, '-');
open(my $out, '>-');
open(my $ok, '<', $path) or die;
open(STDOUT, '>', $path);
open(STDERR, ">&STDOUT");             #~ warning two-arg-open
opendir(DIR, '.');                    #~ warning bareword-filehandle
opendir(my $dh, '.');
pipe(READER, my $writer);             #~ warning bareword-filehandle
$obj->open($path);
