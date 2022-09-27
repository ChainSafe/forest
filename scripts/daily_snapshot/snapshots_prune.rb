# frozen_string_literal: true

require 'date'

# Class representing a snapshot bucket with a defined number of entries.
class SnapshotBucket
  def initialize(max_entries = nil)
    @max_entries = max_entries
    @entries = Set.new
  end

  # Adds an entry to the bucket unless it is already full or already contains the key.
  # Return false on insert failure.
  def add?(entry)
    return false if !@max_entries.nil? && @entries.size >= @max_entries

    !@entries.add?(entry).nil?
  end
end

# Represents Day Bucket. They key is the date.
class DayBucket < SnapshotBucket
  def add?(entry)
    super File.mtime(entry).to_date
  end
end

# Represents Weeks Bucket. The key is "WWYY" (week starts on Monday).
class WeeksBucket < SnapshotBucket
  def add?(entry)
    super File.mtime(entry).to_date.strftime('%m%y')
  end
end

# Represents Months Bucket. The key is "MMYY"
class MonthsBucket < SnapshotBucket
  def add?(entry)
    super File.mtime(entry).to_date.strftime('%m%y')
  end
end

# Prunes snapshots directory with the following retention policy:
# * keep all snapshots generated in the last 7 days,
# * keep one snapshot per week for the last 4 weeks,
# * keep one snapshot per month after 4 weeks.
#
# Returns pruned snapshots' filenames.
def prune_snapshots(snapshots_directory)
  day_bucket = DayBucket.new 7
  weeks_bucket = WeeksBucket.new 4
  months_bucket = MonthsBucket.new
  buckets = [day_bucket, weeks_bucket, months_bucket]

  # iterate over each entry and try to add it to the buckets, newest first.
  Dir.glob(File.join(snapshots_directory, '*.car'))
     .sort_by { |f| File.mtime(f) }
     .reverse
     .reject  { |f| buckets.any? { |bucket| bucket.add? f } }
     .each    { |f| File.delete f }
end
