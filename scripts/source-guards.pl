#!/usr/bin/env perl
# Cheap source guards for the pre-commit hook: file size (warning only), numbers in
# comments, glossary _Avoid_ words. Argument: the repo root (default: the current directory).
use strict;
use warnings;
use File::Find;

my $MAX_LINES = 1000;

my $root = shift // '.';

my @avoid;
open my $glossary, '<', "$root/CONTEXT.md" or die "source-guards: cannot read CONTEXT.md: $!\n";
while (my $line = <$glossary>) {
    next unless $line =~ s/^_Avoid_:\s*//;
    $line =~ s/\([^)]*\)//g;
    push @avoid, grep { length } map { s/^\s+|\s+\z//gr } split /,/, $line;
}
close $glossary;
my $avoid_words = join '|', map { join '[\s-]+', map { quotemeta } split /[\s-]+/ } @avoid;

my $failed = 0;
my @allowed;
my $allow_file = "$root/.glossary-allow";
open my $allow, '<', $allow_file or die "source-guards: cannot read .glossary-allow: $!\n";
while (my $line = <$allow>) {
    next if $line =~ /^\s*(?:#|\z)/;
    my ($fragment, $reason) = $line =~ /^(\S+)\s*(.*?)\s*\z/;
    if ($reason eq '') {
        print STDERR ".glossary-allow:$.: '$fragment' has no reason.\n";
        $failed = 1;
    }
    push @allowed, $fragment;
}
close $allow;

my $allowed_text = join '|', map { quotemeta } sort { length $b <=> length $a } @allowed;
$allowed_text = qr/$allowed_text/ if @allowed;

my @sources;
find(sub { push @sources, $File::Find::name if -f && /\.rs\z/ }, "$root/crates");
@sources = sort @sources;

for my $path (@sources) {
    open my $fh, '<', $path or die "source-guards: cannot read $path: $!\n";
    my $src = do { local $/; <$fh> };
    close $fh;
    my $name = substr $path, length "$root/";

    my $lines = () = $src =~ /\n/g;
    $lines++ if length $src && $src !~ /\n\z/;
    if ($lines > $MAX_LINES) {
        print STDERR "warning: $name: $lines lines, over $MAX_LINES. Run the improve-codebase-architecture skill on it.\n";
    }

    my ($comments, $code) = split_source($src);
    my @code_lines = split /\n/, $code, -1;
    my $number = 0;
    for my $line (split /\n/, $comments, -1) {
        my $text = "$line $code_lines[$number]";
        $number++;
        $text =~ s/$allowed_text/ /g if @allowed;
        $text =~ s/([a-z0-9])([A-Z])/$1 $2/g;
        $text =~ s/_/ /g;
        while (@avoid && $text =~ /\b($avoid_words)\b/gi) {
            print STDERR "$name:$number: '$1' is an _Avoid_ word in CONTEXT.md. Use the glossary term.\n";
            $failed = 1;
        }
        next unless $line =~ /(\bN?FR(?:-\d+)?\b|#\d+|\b[Ss]pec\b)/;
        print STDERR "$name:$number: '$1' in a comment. Tell the rule itself, not where it comes from.\n";
        $failed = 1;
    }
}
exit $failed;

# Splits Rust source into comment text and code text. String and char literals
# go to neither. Both keep every newline, so line numbers stay the same.
sub split_source {
    my ($src) = @_;
    my ($comments, $code) = ('', '');
    while ($src =~ m{\G(?:
            (//[^\n]*|/\*.*?\*/)
          | ((?<![\w])b?r(\#*)".*?"\3|(?<![\w])b?"(?:[^"\\]|\\.)*"|'(?:[^'\\\n]|\\[^\n]{1,10}?)')
          | (\w+|[^/"'\w]+|.)
        )}gsx)
    {
        my ($comment, $literal, $other) = ($1, $2, $4);
        if (defined $comment) {
            $comments .= $comment;
            $code .= blank($comment);
        } elsif (defined $literal) {
            $comments .= blank($literal);
            $code .= blank($literal);
        } else {
            $comments .= blank($other);
            $code .= $other;
        }
    }
    return ($comments, $code);
}

sub blank {
    my ($text) = @_;
    (my $kept = $text) =~ s/[^\n]/ /g;
    return $kept;
}
