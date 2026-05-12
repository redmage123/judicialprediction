"""
Migration 0003 — PasswordResetToken (S5.9).

Adds the single-use, 1-hour-TTL token model that backs the real
password-reset flow (replaces the S4.8 "contact your admin" stub).
"""

import uuid

import django.db.models.deletion
from django.db import migrations, models

import operators.models


class Migration(migrations.Migration):
    dependencies = [
        ("operators", "0003_operator_username_alter_operator_password"),
    ]

    operations = [
        migrations.CreateModel(
            name="PasswordResetToken",
            fields=[
                (
                    "id",
                    models.UUIDField(
                        default=uuid.uuid4,
                        editable=False,
                        primary_key=True,
                        serialize=False,
                    ),
                ),
                (
                    "token",
                    models.CharField(
                        default=operators.models._generate_reset_token,
                        help_text=(
                            "URL-safe CSPRNG value embedded in the reset link."
                        ),
                        max_length=64,
                        unique=True,
                    ),
                ),
                ("created_at", models.DateTimeField(auto_now_add=True)),
                ("expires_at", models.DateTimeField()),
                ("used_at", models.DateTimeField(blank=True, null=True)),
                (
                    "operator",
                    models.ForeignKey(
                        on_delete=django.db.models.deletion.CASCADE,
                        related_name="password_reset_tokens",
                        to="operators.operator",
                    ),
                ),
            ],
            options={
                "verbose_name": "Password reset token",
                "verbose_name_plural": "Password reset tokens",
                "db_table": "operators_passwordresettoken",
                "ordering": ["-created_at"],
                "indexes": [
                    models.Index(
                        fields=["token"],
                        name="passwordresettoken_token_idx",
                    ),
                ],
            },
        ),
    ]
