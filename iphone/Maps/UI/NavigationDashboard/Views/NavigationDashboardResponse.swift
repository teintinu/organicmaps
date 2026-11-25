extension NavigationDashboard {
  enum Response: Equatable {
    case none
    case updateState(MWMNavigationDashboardState, keepElevationPreview: Bool = false)
    case show(points: RoutePoints, routerType: MWMRouterType)

    case updateRouteBuildingProgress(CGFloat, routerType: MWMRouterType)
    case updateNavigationInfo(MWMNavigationDashboardEntity)
    case updateElevationInfo(RouteElevationPreviewData?, activePointDistance: Double?)
    case updateTrackRecordingState(TrackRecordingState)
    case updatePresentationStep(NavigationDashboardModalPresentationStep)
    case updateNavigationInfoAvailableArea(CGRect)
    case updateSearchState(SearchOnMapState)
    case updateDrivingOptionsState(RoutingOptions)
    case setRouteAsTrackSaved

    case showNavigationDashboard
    case setHidden(Bool)
    case showError(String)
    case close
  }
}
