import React, { Suspense, lazy } from 'react';
import { Routes, Route, Navigate, Outlet, useParams } from 'react-router';
import { App } from '@/app/(main)/App';

function LoadingFallback() {
  return (
    <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'center', height: '100%', minHeight: '300px', width: '100%' }}>
      <div style={{ width: '28px', height: '28px', border: '3px solid rgba(125, 125, 125, 0.2)', borderTopColor: '#3b82f6', borderRadius: '50%', animation: 'spin 0.8s linear infinite' }} />
    </div>
  );
}

const DashboardViewPage = lazy(() => import('@/app/(main)/dashboard/DashboardViewPage').then(m => ({ default: m.DashboardViewPage })));
const DashboardEditPage = lazy(() => import('@/app/(main)/dashboard/DashboardEditPage').then(m => ({ default: m.DashboardEditPage })));
const WebsitesPage = lazy(() => import('@/app/(main)/websites/WebsitesPage').then(m => ({ default: m.WebsitesPage })));
const WebsiteLayout = lazy(() => import('@/app/(main)/websites/[websiteId]/WebsiteLayout').then(m => ({ default: m.WebsiteLayout })));
const WebsitePage = lazy(() => import('@/app/(main)/websites/[websiteId]/WebsitePage').then(m => ({ default: m.WebsitePage })));
const RealtimePage = lazy(() => import('@/app/(main)/websites/[websiteId]/realtime/RealtimePage').then(m => ({ default: m.RealtimePage })));
const EventsPage = lazy(() => import('@/app/(main)/websites/[websiteId]/events/EventsPage').then(m => ({ default: m.EventsPage })));
const SessionsPage = lazy(() => import('@/app/(main)/websites/[websiteId]/sessions/SessionsPage').then(m => ({ default: m.SessionsPage })));
const ReplaysPage = lazy(() => import('@/app/(main)/websites/[websiteId]/replays/ReplaysPage').then(m => ({ default: m.ReplaysPage })));
const CohortsPage = lazy(() => import('@/app/(main)/websites/[websiteId]/cohorts/CohortsPage').then(m => ({ default: m.CohortsPage })));
const ComparePage = lazy(() => import('@/app/(main)/websites/[websiteId]/compare/ComparePage').then(m => ({ default: m.ComparePage })));
const SegmentsPage = lazy(() => import('@/app/(main)/websites/[websiteId]/segments/SegmentsPage').then(m => ({ default: m.SegmentsPage })));

const FunnelsPage = lazy(() => import('@/app/(main)/websites/[websiteId]/(reports)/funnels/FunnelsPage').then(m => ({ default: m.FunnelsPage })));
const RetentionPage = lazy(() => import('@/app/(main)/websites/[websiteId]/(reports)/retention/RetentionPage').then(m => ({ default: m.RetentionPage })));
const JourneysPage = lazy(() => import('@/app/(main)/websites/[websiteId]/(reports)/journeys/JourneysPage').then(m => ({ default: m.JourneysPage })));
const HeatmapsPage = lazy(() => import('@/app/(main)/websites/[websiteId]/(reports)/heatmaps/HeatmapsPage').then(m => ({ default: m.HeatmapsPage })));
const AttributionPage = lazy(() => import('@/app/(main)/websites/[websiteId]/(reports)/attribution/AttributionPage').then(m => ({ default: m.AttributionPage })));
const BreakdownPage = lazy(() => import('@/app/(main)/websites/[websiteId]/(reports)/breakdown/BreakdownPage').then(m => ({ default: m.BreakdownPage })));
const GoalsPage = lazy(() => import('@/app/(main)/websites/[websiteId]/(reports)/goals/GoalsPage').then(m => ({ default: m.GoalsPage })));
const PerformancePage = lazy(() => import('@/app/(main)/websites/[websiteId]/(reports)/performance/PerformancePage').then(m => ({ default: m.PerformancePage })));
const RevenuePage = lazy(() => import('@/app/(main)/websites/[websiteId]/(reports)/revenue/RevenuePage').then(m => ({ default: m.RevenuePage })));
const UTMPage = lazy(() => import('@/app/(main)/websites/[websiteId]/(reports)/utm/UTMPage').then(m => ({ default: m.UTMPage })));

const TeamsPage = lazy(() => import('@/app/(main)/teams/TeamsPage').then(m => ({ default: m.TeamsPage })));
const BoardsPage = lazy(() => import('@/app/(main)/boards/BoardsPage').then(m => ({ default: m.BoardsPage })));
const BoardViewPage = lazy(() => import('@/app/(main)/boards/[boardId]/BoardViewPage').then(m => ({ default: m.BoardViewPage })));
const BoardEditPage = lazy(() => import('@/app/(main)/boards/[boardId]/edit/BoardEditPage').then(m => ({ default: m.BoardEditPage })));
const BoardDesignPage = lazy(() => import('@/app/(main)/boards/[boardId]/BoardEditPage').then(m => ({ default: m.BoardDesignPage })));
const LinksPage = lazy(() => import('@/app/(main)/links/LinksPage').then(m => ({ default: m.LinksPage })));
const LinkPage = lazy(() => import('@/app/(main)/links/[linkId]/LinkPage').then(m => ({ default: m.LinkPage })));
const LinkEditPage = lazy(() => import('@/app/(main)/links/[linkId]/edit/LinkEditPage').then(m => ({ default: m.LinkEditPage })));
const PixelsPage = lazy(() => import('@/app/(main)/pixels/PixelsPage').then(m => ({ default: m.PixelsPage })));
const PixelPage = lazy(() => import('@/app/(main)/pixels/[pixelId]/PixelPage').then(m => ({ default: m.PixelPage })));
const PixelEditPage = lazy(() => import('@/app/(main)/pixels/[pixelId]/edit/PixelEditPage').then(m => ({ default: m.PixelEditPage })));

const AdminUsersPage = lazy(() => import('@/app/(main)/admin/users/UsersPage').then(m => ({ default: m.UsersPage })));
const AdminUserDetailPage = lazy(() => import('@/app/(main)/admin/users/[userId]/UserPage').then(m => ({ default: m.UserPage })));
const AdminTeamsPage = lazy(() => import('@/app/(main)/admin/teams/AdminTeamsPage').then(m => ({ default: m.AdminTeamsPage })));
const AdminTeamDetailPage = lazy(() => import('@/app/(main)/admin/teams/[teamId]/AdminTeamPage').then(m => ({ default: m.AdminTeamPage })));
const AdminWebsitesPage = lazy(() => import('@/app/(main)/admin/websites/AdminWebsitesPage').then(m => ({ default: m.AdminWebsitesPage })));
const AdminSecurityPage = lazy(() => import('@/app/(main)/admin/security/AdminSecurityPage').then(m => ({ default: m.AdminSecurityPage })));

const ProfilePage = lazy(() => import('@/app/(main)/settings/profile/ProfilePage').then(m => ({ default: m.ProfilePage })));
const PreferencesPage = lazy(() => import('@/app/(main)/settings/preferences/PreferencesPage').then(m => ({ default: m.PreferencesPage })));
const UserSecurityPage = lazy(() => import('@/app/(main)/settings/security/UserSecurityPage').then(m => ({ default: m.UserSecurityPage })));
const SharePage = lazy(() => import('@/app/share/[slug]/[[...path]]/SharePage').then(m => ({ default: m.SharePage })));
const ShareProvider = lazy(() => import('@/app/share/ShareProvider').then(m => ({ default: m.ShareProvider })));
const TeamsSettingsPage = lazy(() => import('@/app/(main)/settings/teams/TeamsSettingsPage').then(m => ({ default: m.TeamsSettingsPage })));
const TeamSettingsPage = lazy(() => import('@/app/(main)/settings/teams/[teamId]/TeamSettingsPage').then(m => ({ default: m.TeamSettingsPage })));
const WebsitesSettingsPage = lazy(() => import('@/app/(main)/settings/websites/WebsitesSettingsPage').then(m => ({ default: m.WebsitesSettingsPage })));
const SingleWebsiteSettingsPage = lazy(() => import('@/app/(main)/settings/websites/[websiteId]/WebsiteSettingsPage').then(m => ({ default: m.WebsiteSettingsPage })));

const LoginPage = lazy(() => import('@/app/login/LoginPage').then(m => ({ default: m.LoginPage })));
const LogoutPage = lazy(() => import('@/app/logout/LogoutPage').then(m => ({ default: m.LogoutPage })));

function WithWebsite(Component: React.ComponentType<{ websiteId: string }>) {
  return function WebsiteRouteWrapper() {
    const { websiteId = '' } = useParams<{ websiteId: string }>();
    return (
      <Suspense fallback={<LoadingFallback />}>
        <Component websiteId={websiteId} />
      </Suspense>
    );
  };
}

function WithBoard(Component: React.ComponentType<{ boardId: string }>) {
  return function BoardRouteWrapper() {
    const { boardId = '' } = useParams<{ boardId: string }>();
    return (
      <Suspense fallback={<LoadingFallback />}>
        <Component boardId={boardId} />
      </Suspense>
    );
  };
}

function WithLink(Component: React.ComponentType<{ linkId: string }>) {
  return function LinkRouteWrapper() {
    const { linkId = '' } = useParams<{ linkId: string }>();
    return (
      <Suspense fallback={<LoadingFallback />}>
        <Component linkId={linkId} />
      </Suspense>
    );
  };
}

function WithPixel(Component: React.ComponentType<{ pixelId: string }>) {
  return function PixelRouteWrapper() {
    const { pixelId = '' } = useParams<{ pixelId: string }>();
    return (
      <Suspense fallback={<LoadingFallback />}>
        <Component pixelId={pixelId} />
      </Suspense>
    );
  };
}

function WithTeam(Component: React.ComponentType<{ teamId: string }>) {
  return function TeamRouteWrapper() {
    const { teamId = '' } = useParams<{ teamId: string }>();
    return (
      <Suspense fallback={<LoadingFallback />}>
        <Component teamId={teamId} />
      </Suspense>
    );
  };
}

function WithUser(Component: React.ComponentType<{ userId: string }>) {
  return function UserRouteWrapper() {
    const { userId = '' } = useParams<{ userId: string }>();
    return (
      <Suspense fallback={<LoadingFallback />}>
        <Component userId={userId} />
      </Suspense>
    );
  };
}

function WebsiteRouteLayout() {
  const { websiteId = '' } = useParams<{ websiteId: string }>();
  return (
    <Suspense fallback={<LoadingFallback />}>
      <WebsiteLayout websiteId={websiteId}>
        <Outlet />
      </WebsiteLayout>
    </Suspense>
  );
}

const WebsiteMain = WithWebsite(WebsitePage);
const Realtime = WithWebsite(RealtimePage);
const Events = WithWebsite(EventsPage);
const Sessions = WithWebsite(SessionsPage);
const Replays = WithWebsite(ReplaysPage);
const Cohorts = WithWebsite(CohortsPage);
const Compare = WithWebsite(ComparePage);
const Segments = WithWebsite(SegmentsPage);

const Funnels = WithWebsite(FunnelsPage);
const Retention = WithWebsite(RetentionPage);
const Journeys = WithWebsite(JourneysPage);
const Heatmaps = WithWebsite(HeatmapsPage);
const Attribution = WithWebsite(AttributionPage);
const Breakdown = WithWebsite(BreakdownPage);
const Goals = WithWebsite(GoalsPage);
const Performance = WithWebsite(PerformancePage);
const Revenue = WithWebsite(RevenuePage);
const Utm = WithWebsite(UTMPage);

const BoardView = WithBoard(BoardViewPage);
const BoardEdit = WithBoard(BoardEditPage);
const BoardDesign = WithBoard(BoardDesignPage);

const LinkMain = WithLink(LinkPage);
const LinkEdit = WithLink(LinkEditPage);

const PixelMain = WithPixel(PixelPage);
const PixelEdit = WithPixel(PixelEditPage);

const TeamSettingsDetail = WithTeam(TeamSettingsPage);
const WebsiteSettingsDetail = WithWebsite(SingleWebsiteSettingsPage);
const AdminUserDetail = WithUser(AdminUserDetailPage);
const AdminTeamDetail = WithTeam(AdminTeamDetailPage);

function Layout() {
  return (
    <App>
      <Suspense fallback={<LoadingFallback />}>
        <Outlet />
      </Suspense>
    </App>
  );
}

function ShareRouteWrapper() {
  const { slug } = useParams<{ slug: string }>();
  if (!slug) return null;
  return (
    <Suspense fallback={<LoadingFallback />}>
      <ShareProvider slug={slug}>
        <SharePage />
      </ShareProvider>
    </Suspense>
  );
}

export default function AppRoutes() {
  return (
    <Suspense fallback={<LoadingFallback />}>
      <Routes>
        <Route path="/login" element={<LoginPage />} />
        <Route path="/logout" element={<LogoutPage />} />
        <Route path="/share/:slug" element={<ShareRouteWrapper />} />
        <Route path="/share/:slug/*" element={<ShareRouteWrapper />} />
        <Route element={<Layout />}>
          <Route index element={<Navigate to="/dashboard" replace />} />
          <Route path="dashboard" element={<DashboardViewPage />} />
          <Route path="dashboard/edit" element={<DashboardEditPage />} />
          <Route path="websites" element={<WebsitesPage />} />

          <Route path="websites/:websiteId" element={<WebsiteRouteLayout />}>
            <Route index element={<WebsiteMain />} />
            <Route path="realtime" element={<Realtime />} />
            <Route path="events" element={<Events />} />
            <Route path="sessions" element={<Sessions />} />
            <Route path="replays" element={<Replays />} />
            <Route path="cohorts" element={<Cohorts />} />
            <Route path="compare" element={<Compare />} />
            <Route path="segments" element={<Segments />} />

            <Route path="performance" element={<Performance />} />
            <Route path="breakdown" element={<Breakdown />} />
            <Route path="goals" element={<Goals />} />
            <Route path="funnels" element={<Funnels />} />
            <Route path="journeys" element={<Journeys />} />
            <Route path="retention" element={<Retention />} />
            <Route path="heatmaps" element={<Heatmaps />} />
            <Route path="revenue" element={<Revenue />} />
            <Route path="attribution" element={<Attribution />} />
            <Route path="utm" element={<Utm />} />

            <Route path="reports/funnels" element={<Funnels />} />
            <Route path="reports/retention" element={<Retention />} />
            <Route path="reports/journeys" element={<Journeys />} />
            <Route path="reports/heatmaps" element={<Heatmaps />} />
            <Route path="reports/attribution" element={<Attribution />} />
            <Route path="reports/breakdown" element={<Breakdown />} />
            <Route path="reports/goals" element={<Goals />} />
            <Route path="reports/performance" element={<Performance />} />
            <Route path="reports/revenue" element={<Revenue />} />
            <Route path="reports/utm" element={<Utm />} />
          </Route>

          <Route path="teams" element={<TeamsPage />} />
          <Route path="boards" element={<BoardsPage />} />
          <Route path="boards/:boardId" element={<BoardView />} />
          <Route path="boards/:boardId/edit" element={<BoardEdit />} />
          <Route path="boards/:boardId/design" element={<BoardDesign />} />

          <Route path="links" element={<LinksPage />} />
          <Route path="links/:linkId" element={<LinkMain />} />
          <Route path="links/:linkId/edit" element={<LinkEdit />} />

          <Route path="pixels" element={<PixelsPage />} />
          <Route path="pixels/:pixelId" element={<PixelMain />} />
          <Route path="pixels/:pixelId/edit" element={<PixelEdit />} />

          <Route path="admin" element={<Navigate to="/admin/users" replace />} />
          <Route path="admin/users" element={<AdminUsersPage />} />
          <Route path="admin/users/:userId" element={<AdminUserDetail />} />
          <Route path="admin/teams" element={<AdminTeamsPage />} />
          <Route path="admin/teams/:teamId" element={<AdminTeamDetail />} />
          <Route path="admin/websites" element={<AdminWebsitesPage />} />
          <Route path="admin/security" element={<AdminSecurityPage />} />

          <Route path="settings" element={<Navigate to="/settings/profile" replace />} />
          <Route path="settings/profile" element={<ProfilePage />} />
          <Route path="settings/preferences" element={<PreferencesPage />} />
          <Route path="settings/security" element={<UserSecurityPage />} />
          <Route path="settings/teams" element={<TeamsSettingsPage />} />
          <Route path="settings/teams/:teamId" element={<TeamSettingsDetail />} />
          <Route path="settings/websites" element={<WebsitesSettingsPage teamId="" />} />
          <Route path="settings/websites/:websiteId" element={<WebsiteSettingsDetail />} />
        </Route>
      </Routes>
    </Suspense>
  );
}
