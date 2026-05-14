from django.contrib import admin
from django.urls import path

from operators import oidc_views as operator_oidc_views
from operators import views as operator_views

admin.site.site_header = "JudicialPredict Operator Console"
admin.site.site_title = "JudicialPredict Admin"
admin.site.index_title = "Operator Dashboard"

urlpatterns = [
    path("admin/", admin.site.urls),
    # Auth endpoints consumed by the Next.js BFF proxy (S4.8).
    path("api/auth/login", operator_views.login, name="auth-login"),
    path("api/auth/logout", operator_views.logout, name="auth-logout"),
    # S5.9 — password reset (replaces the S4.8 "contact your admin" stub).
    path(
        "api/auth/reset/request",
        operator_views.password_reset_request,
        name="auth-reset-request",
    ),
    path(
        "api/auth/reset/confirm",
        operator_views.password_reset_confirm,
        name="auth-reset-confirm",
    ),
    # S6.6 — OIDC SSO (env-gated; endpoints 404 when OIDC_ENABLED is false).
    path(
        "api/auth/sso/config",
        operator_oidc_views.sso_config,
        name="auth-sso-config",
    ),
    path(
        "api/auth/sso/login",
        operator_oidc_views.sso_login,
        name="auth-sso-login",
    ),
    path(
        "api/auth/sso/callback",
        operator_oidc_views.sso_callback,
        name="auth-sso-callback",
    ),
]
