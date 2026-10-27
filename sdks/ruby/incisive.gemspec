require_relative "lib/incisive/version"

Gem::Specification.new do |s|
  s.name        = "incisive"
  s.version     = Incisive::VERSION
  s.summary     = "Ruby client for the Incisive analytics server"
  s.description = "Publish events and run analysis queries against a self-hosted Incisive server."
  s.authors     = ["Incisive contributors"]
  s.license     = "MIT"
  s.files       = Dir["lib/**/*.rb", "README.md"]
  s.require_paths = ["lib"]
  s.required_ruby_version = ">= 2.7"
end
