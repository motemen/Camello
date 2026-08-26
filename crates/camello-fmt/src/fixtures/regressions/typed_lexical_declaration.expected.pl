# `my CLASS $var` declares $var; the class name in front of it is advisory
# (perlsub). Proc::Daemon and TheSchwartz both open every method with one.
package Proc::Daemon;

sub new { bless {}, shift }

package main;

sub Init {
    my Proc::Daemon $self = shift;
    my $settings_ref      = shift;
    return $self;
}
