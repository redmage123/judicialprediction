from django.contrib import admin
from django.urls import path

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
]
