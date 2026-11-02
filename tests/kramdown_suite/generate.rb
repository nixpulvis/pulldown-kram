# Regenerates the expected HTML of kramdown's own test cases, as kramdown
# reads them the way Jekyll runs it, for tests/kramdown_suite/main.rs.
#
#     ruby tests/kramdown_suite/generate.rb
#
# The inputs in testcases/ are the `.text` files of kramdown 2.5.2's
# test/testcases, with their `.options` files and the directory-level
# `options` file, copied unchanged. kramdown is MIT licensed; its notice is
# in COPYING here.
#
# Each case is converted with kramdown 2.5.2 and kramdown-parser-gfm 1.1.0
# to `<case>.html`, beside its `.text`, with these options:
#
# 1. The case's options, as kramdown's runner (test/test_files.rb) finds
#    them: the case's `.options` file, or else an `options` file in its
#    directory, or else `{auto_ids: false, footnote_nr: 1}`.
# 2. With kramdown's default for each option pulldown-kram doesn't
#    support, in UNSUPPORTED below, in place of the case's own.
# 3. Merged over those, always:
#    - `input: 'GFM'`, kramdown-parser-gfm's parser, as Jekyll reads.
#    - `hard_wrap: false`, as Jekyll sets it, so newlines in a paragraph
#      stay soft.
#    - `syntax_highlighter: nil`, so code blocks and spans are plain
#      `<pre><code class="language-…">`, without a highlighter's markup.
#
# Everything else is kramdown's default. Warnings kramdown gives are
# ignored, as its runner ignores them.
#
# It writes cases.txt, the cases in order, a line each with tabs between:
# the case, the extensions to read it without, and the unsupported options
# it gives that kramdown's defaults stood in for, each list split by
# commas, or `-` if empty.
#
# It also writes entities.txt, kramdown's named entities, a name and a code
# point on each line, for the test to decode what kramdown writes.
#
# Every option a case gives must be in one of the tables below; one that
# isn't stops this, to be put in one. When pulldown-kram comes to support
# an option with an extension, move it from UNSUPPORTED to EXTENSIONS and
# run this again: its cases are then converted with it, and read with or
# without the extension as the option says.

gem 'kramdown', '2.5.2'
gem 'kramdown-parser-gfm', '1.1.0'
require 'kramdown'
require 'kramdown-parser-gfm'
require 'yaml'

# Options pulldown-kram follows with an extension, on when the option is,
# as the case gives it or else by kramdown's default.
EXTENSIONS = {
  auto_ids: 'ENABLE_AUTO_IDS',
}.freeze

# Options that change nothing the test compares, given to kramdown as they
# are.
KEPT = {
  entity_output: 'how characters are written, which the test erases',
  auto_id_stripping: 'always on with GFM input',
  transliterated_header_ids: 'ignored by GFM input',
  syntax_highlighter: 'turned off, below',
  syntax_highlighter_opts: 'for the highlighter, which is off',
  enable_coderay: "kramdown 1's, ignored by kramdown 2",
  coderay_wrap: "kramdown 1's, ignored by kramdown 2",
  coderay_line_numbers: "kramdown 1's, ignored by kramdown 2",
  coderay_default_lang: "kramdown 1's, ignored by kramdown 2",
  coderay_css: "kramdown 1's, ignored by kramdown 2",
}.freeze

# Options pulldown-kram doesn't support, and why. Cases are converted with
# kramdown's default for each instead of what they give.
UNSUPPORTED = {
  parse_block_html: 'HTML blocks are never read as Markdown',
  html_to_native: 'HTML is kept as HTML events',
  math_engine: 'math is always written for MathJax',
  link_defs: 'there are no predefined link definitions',
  smart_quotes: 'quotes are always made typographic the same way',
  typographic_symbols: 'symbols are always made the same way',
  toc_levels: 'tables of contents list every level',
  auto_id_prefix: 'header ids have no prefix',
  footnote_nr: 'the HTML writer numbers footnotes from 1',
  footnote_prefix: 'the HTML writer has no prefix for footnote ids',
  footnote_link_text: "the HTML writer writes a footnote's number alone",
  footnote_backlink: 'the HTML writer always links back with ↩',
  footnote_backlink_inline: 'the HTML writer always links back in the last paragraph',
  header_offset: 'the HTML writer writes headings at their own level',
  header_links: "the HTML writer doesn't link headings to themselves",
  remove_line_breaks_for_cjk: 'the HTML writer keeps line breaks',
}.freeze

FORCED = { input: 'GFM', hard_wrap: false, syntax_highlighter: nil }.freeze
DEFAULT = { auto_ids: false, footnote_nr: 1 }.freeze

root = File.join(__dir__, 'testcases')
cases = Dir[File.join(root, '**', '*.text')].sort
manifest = cases.map do |text_file|
  options_file = text_file.sub(/\.text\z/, '.options')
  options_file = File.join(File.dirname(text_file), 'options') unless File.exist?(options_file)
  options = File.exist?(options_file) ? YAML.load(File.read(options_file)) : DEFAULT
  options = options.transform_keys(&:to_sym)

  name = text_file.delete_prefix("#{root}/").delete_suffix('.text')
  unknown = options.keys - EXTENSIONS.keys - KEPT.keys - UNSUPPORTED.keys
  abort "#{name}: put #{unknown.join(', ')} in a table in #{__FILE__}" unless unknown.empty?
  # An option given its default already is the same either way, so isn't
  # listed.
  defaulted = (options.keys & UNSUPPORTED.keys)
    .reject { |option| options[option] == Kramdown::Options.defaults[option] }
  options = options.except(*UNSUPPORTED.keys).merge(FORCED)

  without = EXTENSIONS.filter_map do |option, extension|
    extension unless options.fetch(option) { Kramdown::Options.defaults[option] }
  end

  document = Kramdown::Document.new(File.read(text_file, encoding: 'utf-8'), options)
  File.write(text_file.sub(/\.text\z/, '.html'), document.to_html)
  list = ->(items) { items.empty? ? '-' : items.join(',') }
  "#{name}\t#{list[without]}\t#{list[defaulted]}\n"
end
File.write(File.join(__dir__, 'cases.txt'), manifest.join)
puts "#{cases.size} cases written"

entities = Kramdown::Utils::Entities::ENTITY_TABLE
  .select { |_, name| name.is_a?(String) }
  .map { |code_point, name| "#{name} #{code_point}\n" }
File.write(File.join(__dir__, 'entities.txt'), entities.join)
