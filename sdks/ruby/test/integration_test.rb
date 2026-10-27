require "test_helper"
require "securerandom"

# Runs against a real server only when INCISIVE_BASE_URL and
# INCISIVE_PROJECT_ID are set (plus the keys each test needs).
class IntegrationTest < Minitest::Test
  def setup
    unless ENV["INCISIVE_BASE_URL"] && ENV["INCISIVE_PROJECT_ID"]
      skip "set INCISIVE_BASE_URL and INCISIVE_PROJECT_ID to run integration tests"
    end
    @client = Incisive::Client.from_env
    @collection = "ruby_sdk_it_#{SecureRandom.hex(4)}"
  end

  def need(*keys)
    missing = keys.reject { |k| ENV["INCISIVE_#{k.upcase}_KEY"] }
    skip "missing #{missing.join(', ')} key(s)" unless missing.empty?
  end

  def eventually(tries = 20)
    last = nil
    tries.times do
      last = yield
      return last if last
      sleep 0.5
    end
    last
  end

  def test_publish_then_count
    need(:write, :read)
    assert @client.publish(@collection, { "n" => 1, "who" => "a" })
    res = @client.publish_batch(@collection => [{ "n" => 2, "who" => "b" }, { "n" => 3, "who" => "a" }])
    assert_kind_of Hash, res
    count = eventually { c = @client.count(@collection, timeframe: "this_1_days"); c == 3 ? c : nil }
    assert_equal 3, count
    assert_equal 6, @client.sum(@collection, target_property: "n", timeframe: "this_1_days")
    assert_equal 2, @client.count_unique(@collection, target_property: "who", timeframe: "this_1_days")
    filtered = @client.count(@collection, timeframe: "this_1_days",
                                          filters: [{ property_name: "who", operator: "eq", property_value: "a" }])
    assert_equal 2, filtered
  end

  def test_delete_collection
    need(:write, :master)
    @client.publish(@collection, { "n" => 1 })
    assert @client.delete(@collection)
  end
end
