#pragma once

#include "routing/track_router.hpp"

#include "kml/types.hpp"

#include <compare>
#include <optional>

class Track;

struct TrackSourcePosition
{
  size_t m_line = 0;
  size_t m_segment = 0;
  double m_fraction = 0.0;

  auto operator<=>(TrackSourcePosition const &) const = default;
};

// Resolve the selected source occurrence using both its coordinate and finite chart distance.
std::optional<TrackSourcePosition> ResolveTrackEntry(Track const & track, m2::PointD const & selectedPoint,
                                                     double distanceFromBeginningM);

// The caller supplies a finite distance in [0, track length]. A gap uses the preceding section.
TrackSourcePosition ResolveTrackEntryAt(Track const & track, double distanceFromBeginningM);

// Whether horizontal geometry remains in the requested direction from a valid source position, without preparing stops.
bool HasRemainingTrackGeometry(kml::MultiGeometry const & geometry, TrackSourcePosition const & entry,
                               bool reverse = false);

// Prepare the connected remainder and its stops in travel order, preserving altitude availability.
// Returns nullopt when no horizontal geometry remains in that direction.
std::optional<routing::TrackFollowingData> BuildTrackFollowingData(kml::TrackData const & source,
                                                                   TrackSourcePosition const & entry,
                                                                   bool reverse = false);
