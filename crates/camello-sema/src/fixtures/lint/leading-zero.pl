use strict;
use warnings;

my $mode = 0755;          #~ warning leading-zero: 493 in decimal
my $list = [ 01, 2 ];     #~ warning leading-zero
chmod 0755, 'file';
chmod(0644, 'file');
mkdir 'dir', 0700;
umask 022;
sysopen(my $fh, 'f', 0, 0600);
my @ok = (0, 0.5, 0x1f, 0b101, 10, 1_000);
my $perm = (stat 'f')[2] & 07777;
my $exec = 0444 | ($perm & 0111 ? 0111 : 0);
$perm &= 0755;
File::Path::mkpath(['dir'], 0, 0755);
_chmod(0666, 'file');
sub _chmod { 1 }
